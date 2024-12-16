use super::errors::{
    AccountLockError, ChargebackError, DepositError, DisputeError, ResolveError, TransactionErrors,
    WithdrawalError,
};
use crate::{
    models::{Account, TranactionState, Transaction, TransactionDetail},
    tranasction::errors::DuplicateTransactionError,
};
use ahash::AHashMap;
use anyhow::bail;
use rust_decimal_macros::dec;
use std::io::BufWriter;
use tokio::sync::mpsc::Receiver;

const TRANSACTION_MAP_SIZE: usize = 1000000;
const ACCOUNT_MAP_SIZE: usize = 1000000;

pub struct TransactionEngine {
    rx: Receiver<Transaction>,
    //map that stores all the deposit and withdrawal transactions
    withdrawal_transactions: AHashMap<u32, TransactionDetail>,
    deposit_transactions: AHashMap<u32, TransactionDetail>,
    accounts: AHashMap<u16, Account>,
}

impl TransactionEngine {
    pub fn new(rx: Receiver<Transaction>) -> Self {
        Self {
            rx,
            withdrawal_transactions: AHashMap::with_capacity(TRANSACTION_MAP_SIZE),
            deposit_transactions: AHashMap::with_capacity(TRANSACTION_MAP_SIZE),
            accounts: AHashMap::with_capacity(ACCOUNT_MAP_SIZE),
        }
    }

    fn process_transaction(&mut self, tx: Transaction) {
        match tx {
            Transaction::Deposit(tx_detail) => {
                if let Err(e) = self.process_deposit(tx_detail) {
                    tracing::error!("Fail to deposit: {e:?}");
                }
            }
            Transaction::Withdrawal(tx_detail) => {
                if let Err(e) = self.process_withdrawal(tx_detail) {
                    tracing::error!("Fail to withdraw: {e:?}");
                }
            }
            Transaction::Dispute(tx_detail) => {
                if let Err(e) = self.process_dispute(tx_detail) {
                    tracing::error!("Fail to dispute: {e:?}");
                }
            }
            Transaction::Resolve(tx_detail) => {
                if let Err(e) = self.process_resolve(tx_detail) {
                    tracing::error!("Fail to resolve: {e:?}");
                }
            }
            Transaction::ChargeBack(tx_detail) => {
                if let Err(e) = self.process_chargeback(tx_detail) {
                    tracing::error!("Fail to chargeback: {e:?}");
                }
            }
            //ignore unknown transaction
            Transaction::Unknown => {
                tracing::error!("Skipped unknown transaction");
            }
        }
    }

    fn get_unlocked_account(
        accounts: &mut AHashMap<u16, Account>,
        client: u16,
    ) -> anyhow::Result<&mut Account> {
        let account = accounts.entry(client).or_insert(Account::new(client));
        if account.locked {
            bail!(TransactionErrors::AccountLock(AccountLockError { client },))
        } else {
            Ok(account)
        }
    }

    // helper function to check if transaction id already exists
    fn check_dup_transaction_id(
        transactions: &AHashMap<u32, TransactionDetail>,
        tx: u32,
    ) -> anyhow::Result<()> {
        if transactions.get(&tx).is_some() {
            bail!(TransactionErrors::DuplicateTransaction(
                DuplicateTransactionError { tx },
            ))
        }
        Ok(())
    }

    fn process_deposit(&mut self, tx_detail: TransactionDetail) -> anyhow::Result<()> {
        let _ = Self::check_dup_transaction_id(&self.deposit_transactions, tx_detail.tx)?;
        if let Some(amount) = tx_detail.amount {
            if amount > dec!(0) {
                let account = Self::get_unlocked_account(&mut self.accounts, tx_detail.client)?;
                account.available += amount;
                account.total += amount;
                self.deposit_transactions.insert(tx_detail.tx, tx_detail);
                return Ok(());
            }
        }

        bail!(TransactionErrors::Deposit(DepositError {
            tx: tx_detail.tx
        },))
        /*Err(anyhow::Error::new(TransactionErrors::Deposit(
            DepositError { tx: tx_detail.tx },
        )))*/
    }

    fn process_withdrawal(&mut self, tx_detail: TransactionDetail) -> anyhow::Result<()> {
        let _ = Self::check_dup_transaction_id(&self.withdrawal_transactions, tx_detail.tx)?;
        if let Some(amount) = tx_detail.amount {
            let account = Self::get_unlocked_account(&mut self.accounts, tx_detail.client)?;
            //if the amount is > 0 and if available fund is > the withdraw amount
            if amount > dec!(0) && account.available >= amount {
                account.available -= amount;
                account.total -= amount;
                self.withdrawal_transactions.insert(tx_detail.tx, tx_detail);
                return Ok(());
            }
        }

        bail!(TransactionErrors::Withdrawal(WithdrawalError {
            tx: tx_detail.tx
        },))
    }

    //The doc mentioned that during a dispute, the held fund is increased by the dispute amount and the available fund is decreased by. I assume that
    //this is referring to a dispute for a withdrawal transaction as it simply means moving fund from the the available fund to the held fund. For disputing a
    // withdrawal, I don't think we should decrease the avaiable fund as the client as disputing an incorrect amount being debit from his/her account. So for the dispute
    //of a withdrawal transaction, I decided to increment the held fund only, which means the total fund will increase. However, since the client can't really use that amount yet,
    //so I believe it's fine.
    fn process_dispute(&mut self, tx_detail: TransactionDetail) -> anyhow::Result<()> {
        //ignore the dispute if the account is locked
        let account = Self::get_unlocked_account(&mut self.accounts, tx_detail.client)?;
        //if the dispute transaction is a deposit
        if let Some(dispute_tx_detail) = self.deposit_transactions.get_mut(&tx_detail.tx) {
            if let Some(amount) = dispute_tx_detail.amount {
                if tx_detail.client == dispute_tx_detail.client
                    && dispute_tx_detail.state == TranactionState::Normal
                    && account.available >= amount
                {
                    account.available -= amount;
                    account.held += amount;
                    dispute_tx_detail.state = TranactionState::Dispute;
                    return Ok(());
                }
            }
        }
        //if the dispute transaction is a withdraw
        else if let Some(dispute_tx_detail) = self.withdrawal_transactions.get_mut(&tx_detail.tx)
        {
            if let Some(amount) = dispute_tx_detail.amount {
                if tx_detail.client == dispute_tx_detail.client
                    && dispute_tx_detail.state == TranactionState::Normal
                {
                    account.held += amount;
                    account.total += amount;
                    dispute_tx_detail.state = TranactionState::Dispute;
                    return Ok(());
                }
            }
        }

        bail!(TransactionErrors::Dispute(DisputeError {
            tx: tx_detail.tx
        },))
    }

    fn process_resolve(&mut self, tx_detail: TransactionDetail) -> anyhow::Result<()> {
        //ignore the resolve if the account is locked
        let account = Self::get_unlocked_account(&mut self.accounts, tx_detail.client)?;

        if let Some(resolve_tx_detail) = self
            .deposit_transactions
            .get_mut(&tx_detail.tx)
            .or_else(|| self.withdrawal_transactions.get_mut(&tx_detail.tx))
        {
            if let Some(amount) = resolve_tx_detail.amount {
                if tx_detail.client == resolve_tx_detail.client
                    && resolve_tx_detail.state == TranactionState::Dispute
                    && account.held >= amount
                {
                    account.held -= amount;
                    account.available += amount;
                    resolve_tx_detail.state = TranactionState::Resolve;
                    return Ok(());
                }
            }
        }

        bail!(TransactionErrors::Resolve(ResolveError {
            tx: tx_detail.tx
        },))
    }

    fn process_chargeback(&mut self, tx_detail: TransactionDetail) -> anyhow::Result<()> {
        //ignore the chargeback if the account is locked
        let account = Self::get_unlocked_account(&mut self.accounts, tx_detail.client)?;
        if let Some(chargeback_tx_detail) = self
            .deposit_transactions
            .get_mut(&tx_detail.tx)
            .or_else(|| self.withdrawal_transactions.get_mut(&tx_detail.tx))
        {
            if let Some(amount) = chargeback_tx_detail.amount {
                if tx_detail.client == chargeback_tx_detail.client
                    && chargeback_tx_detail.state == TranactionState::Dispute
                    && account.held >= amount
                {
                    account.held -= amount;
                    chargeback_tx_detail.state = TranactionState::ChargeBack;
                    //lock the account
                    account.locked = true;
                    return Ok(());
                }
            }
        }
        bail!(TransactionErrors::Chargeback(ChargebackError {
            tx: tx_detail.tx
        },))
    }

    fn output(&self) {
        let writer = BufWriter::new(std::io::stdout());
        let mut wtr = csv::Writer::from_writer(writer);
        self.accounts.values().for_each(|account| {
            if let Err(e) = wtr.serialize(account.clone()) {
                tracing::error!("Fail to write: {e}");
            }
        });
    }

    pub async fn run(&mut self) {
        while let Some(transaction) = self.rx.recv().await {
            tracing::info!("Got {:?}", transaction);
            self.process_transaction(transaction);
        }

        self.output();
    }
}

#[cfg(test)]
mod tests {
    use super::TransactionEngine;
    use crate::models::Transaction::{Deposit, Dispute, Resolve, Withdrawal};
    use crate::models::{TranactionState, TransactionDetail};
    use assert_approx_eq::assert_approx_eq;
    use rust_decimal::prelude::*;
    use rust_decimal_macros::dec;
    use tokio::sync::mpsc;

    fn get_transaction_engine() -> TransactionEngine {
        let (_, rx) = mpsc::channel(10);
        TransactionEngine::new(rx)
    }

    fn check_account(
        engine: &TransactionEngine,
        account_id: u16,
        available: f64,
        held: f64,
        total: f64,
        deposits: usize,
        withdraws: usize,
    ) {
        let account = engine.accounts.get(&account_id).unwrap();
        assert_approx_eq!(account.available.to_f64().unwrap(), available);
        assert_approx_eq!(account.total.to_f64().unwrap(), total);
        assert_approx_eq!(account.held.to_f64().unwrap(), held);
        assert_eq!(engine.deposit_transactions.len(), deposits);
        assert_eq!(engine.withdrawal_transactions.len(), withdraws);
    }

    fn check_transaction(engine: &TransactionEngine, tx: u32, state: TranactionState) {
        let transaction = engine
            .deposit_transactions
            .get(&tx)
            .or_else(|| engine.withdrawal_transactions.get(&tx))
            .unwrap();

        assert_eq!(transaction.state, state);
    }

    #[test]
    fn test_deposit_and_withdraw() {
        let mut engine = get_transaction_engine();
        //invalid deposit transaction
        let tx = TransactionDetail::new(1, 2, None);
        assert_eq!(
            format!("{}", engine.process_deposit(tx).unwrap_err()),
            "Deposit error for tx 2"
        );
        assert!(engine.accounts.is_empty(),);
        assert!(engine.deposit_transactions.is_empty(),);
        assert!(engine.withdrawal_transactions.is_empty(),);

        //a valid transaction for client 1
        let tx = TransactionDetail::new(1, 2, Some(dec!(1.1111)));
        let _ = engine.process_deposit(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 1, 0);

        //Dup transaction id
        let tx = TransactionDetail::new(1, 2, Some(dec!(2.01)));
        assert_eq!(
            format!("{}", engine.process_deposit(tx).unwrap_err()),
            "Duplicate transaction id 2"
        );

        //a valid transaction for client 1
        let tx = TransactionDetail::new(1, 3, Some(dec!(1.8889)));
        let _ = engine.process_deposit(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 3.0, 0_f64, 3.0, 2, 0);

        //an invalid withdraw
        let tx = TransactionDetail::new(1, 4, None);
        assert_eq!(
            format!("{}", engine.process_withdrawal(tx).unwrap_err()),
            "Withdraw error for tx 4"
        );
        assert!(engine.withdrawal_transactions.is_empty(),);

        //a valid withdraw
        let tx = TransactionDetail::new(1, 4, Some(dec!(1.05)));
        let _ = engine.process_withdrawal(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 1.95, 0_f64, 1.95, 2, 1);

        //an invalid withdraw with dup transaction id
        let tx = TransactionDetail::new(1, 4, Some(dec!(1.95)));
        assert_eq!(
            format!("{}", engine.process_withdrawal(tx).unwrap_err()),
            "Duplicate transaction id 4"
        );
        check_account(&engine, 1, 1.95, 0_f64, 1.95, 2, 1);

        //Withdraw more than available
        let tx = TransactionDetail::new(1, 5, Some(dec!(1.96)));
        assert_eq!(
            format!("{}", engine.process_withdrawal(tx).unwrap_err()),
            "Withdraw error for tx 5"
        );
        check_account(&engine, 1, 1.95, 0_f64, 1.95, 2, 1);

        //Withdraw everything
        let tx = TransactionDetail::new(1, 5, Some(dec!(1.95)));
        let _ = engine.process_withdrawal(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 0_f64, 0_f64, 0_f64, 2, 2);
    }

    #[test]
    fn test_multiple_account_withdraw_() {
        let mut engine = get_transaction_engine();
        //a deposit for client 1
        let tx = Deposit(TransactionDetail::new(1, 1, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 1, 0);

        //a deposit for client 2
        let tx = Deposit(TransactionDetail::new(2, 2, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0);

        //a deposit for client 3
        let tx = Deposit(TransactionDetail::new(3, 3, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 3);
        check_account(&engine, 3, 1.1111, 0_f64, 1.1111, 3, 0);

        //a failed withdraw for client 4
        let tx = TransactionDetail::new(4, 4, Some(dec!(1.1111)));
        assert_eq!(
            format!("{}", engine.process_withdrawal(tx).unwrap_err()),
            "Withdraw error for tx 4"
        );

        //a withdraw for client 3
        let tx = Withdrawal(TransactionDetail::new(3, 5, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 4);
        check_account(&engine, 3, 0_f64, 0_f64, 0_f64, 3, 1);

        //a withdraw for client 2
        let tx = Withdrawal(TransactionDetail::new(2, 6, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 4);
        check_account(&engine, 2, 0_f64, 0_f64, 0_f64, 3, 2);

        //a withdraw for client 1
        let tx = Withdrawal(TransactionDetail::new(1, 7, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 4);
        check_account(&engine, 1, 0_f64, 0_f64, 0_f64, 3, 3);
    }

    #[test]
    fn test_deposit_dispute_resolve() {
        let mut engine = get_transaction_engine();
        //a deposit for client 1
        let tx = Deposit(TransactionDetail::new(1, 1, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 1, 0);

        //a deposit for client 2
        let tx = Deposit(TransactionDetail::new(2, 2, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0);

        //invalid dispute as transaction doesn't exist
        let tx = TransactionDetail::new(1, 3, None);
        assert_eq!(
            format!("{}", engine.process_dispute(tx).unwrap_err()),
            "Dispute error for tx 3"
        );

        //invalid dispute as client is incorrect
        let tx = TransactionDetail::new(2, 1, None);
        assert_eq!(
            format!("{}", engine.process_dispute(tx).unwrap_err()),
            "Dispute error for tx 1"
        );

        //valid dispute for client 1
        let tx = Dispute(TransactionDetail::new(1, 1, None));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 1, 0_f64, 1.1111, 1.1111, 2, 0);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0);
        check_transaction(&engine, 1, TranactionState::Dispute);

        //invalid resolve as transaction doesn't exist
        let tx = TransactionDetail::new(1, 3, None);
        assert_eq!(
            format!("{}", engine.process_resolve(tx).unwrap_err()),
            "Resolve error for tx 3"
        );

        //invalid resolve as client is incorrect
        let tx = TransactionDetail::new(2, 1, None);
        assert_eq!(
            format!("{}", engine.process_resolve(tx).unwrap_err()),
            "Resolve error for tx 1"
        );

        //invalid resolve as transaction is not in dispute state
        let tx = TransactionDetail::new(2, 2, None);
        assert_eq!(
            format!("{}", engine.process_resolve(tx).unwrap_err()),
            "Resolve error for tx 2"
        );

        //valid resolve for client 1
        let tx = Resolve(TransactionDetail::new(1, 1, None));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 2, 0);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0);
    }

    #[test]
    fn test_withdraw_dispute_resolve() {
        let mut engine = get_transaction_engine();
        //a deposit for client 1
        let tx = Deposit(TransactionDetail::new(1, 1, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 1, 0);

        //a deposit for client 2
        let tx = Deposit(TransactionDetail::new(2, 2, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0);

        //a withdraw for client 2
        let tx = Withdrawal(TransactionDetail::new(1, 3, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 1, 0_f64, 0_f64, 0_f64, 2, 1);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 1);

        //invalid dispute as transaction doesn't exist
        let tx = TransactionDetail::new(1, 4, None);
        assert_eq!(
            format!("{}", engine.process_dispute(tx).unwrap_err()),
            "Dispute error for tx 4"
        );

        //invalid dispute as client is incorrect
        let tx = TransactionDetail::new(2, 3, None);
        assert_eq!(
            format!("{}", engine.process_dispute(tx).unwrap_err()),
            "Dispute error for tx 3"
        );

        //valid dispute for client 1
        let tx = Dispute(TransactionDetail::new(1, 3, None));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 1, 0_f64, 1.1111, 1.1111, 2, 1);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 1);
        check_transaction(&engine, 3, TranactionState::Dispute);

        //invalid resolve as transaction doesn't exist
        let tx = TransactionDetail::new(1, 4, None);
        assert_eq!(
            format!("{}", engine.process_resolve(tx).unwrap_err()),
            "Resolve error for tx 4"
        );

        //invalid resolve as client is incorrect
        let tx = TransactionDetail::new(2, 3, None);
        assert_eq!(
            format!("{}", engine.process_resolve(tx).unwrap_err()),
            "Resolve error for tx 3"
        );

        //valid resolve for client 1
        let tx = Resolve(TransactionDetail::new(1, 3, None));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 2, 1);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 1);
    }
}
