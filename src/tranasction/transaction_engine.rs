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
use std::io::BufWriter;
use tokio::sync::mpsc::Receiver;

const TRANSACTION_MAP_SIZE: usize = 10000;
//client id is u16
const ACCOUNT_MAP_SIZE: usize = u16::MAX as usize;

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
        Self::check_dup_transaction_id(&self.deposit_transactions, tx_detail.tx)?;
        if let Some(amount) = tx_detail.amount {
            if amount > 0.0 {
                let account = Self::get_unlocked_account(&mut self.accounts, tx_detail.client)?;
                account.available += amount;
                account.total += amount;
                if self
                    .deposit_transactions
                    .insert(tx_detail.tx, tx_detail)
                    .is_none()
                {
                    //if map is full, try to resesrve additional space
                    if self.deposit_transactions.len() == self.deposit_transactions.capacity() {
                        if let Err(e) = self.deposit_transactions.try_reserve(TRANSACTION_MAP_SIZE)
                        {
                            tracing::error!(
                                "Fail to reserve capacity for the deposit transaction map: {e}"
                            );
                        }
                    }
                }
                return Ok(());
            }
        }

        bail!(TransactionErrors::Deposit(DepositError {
            tx: tx_detail.tx
        },))
    }

    fn process_withdrawal(&mut self, tx_detail: TransactionDetail) -> anyhow::Result<()> {
        Self::check_dup_transaction_id(&self.withdrawal_transactions, tx_detail.tx)?;
        if let Some(amount) = tx_detail.amount {
            let account = Self::get_unlocked_account(&mut self.accounts, tx_detail.client)?;
            //if the amount is > 0 and if available fund is > the withdraw amount
            if amount > 0.0 && account.available >= amount {
                account.available -= amount;
                account.total -= amount;
                if self
                    .withdrawal_transactions
                    .insert(tx_detail.tx, tx_detail)
                    .is_none()
                {
                    //if map is full, try to resesrve additional space
                    if self.withdrawal_transactions.len() == self.withdrawal_transactions.capacity()
                    {
                        if let Err(e) = self
                            .withdrawal_transactions
                            .try_reserve(TRANSACTION_MAP_SIZE)
                        {
                            tracing::error!(
                                "Fail to reserve capacity for the withdrawal transaction map: {e}"
                            );
                        }
                    }
                }
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
                    //Move the dispute amount from available to held, total doesn't change
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
                    //increase the held and total. Since the increased amount is held, increasing the total should be
                    //fine
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

        //resolve disputed deposit transaction
        if let Some(resolve_tx_detail) = self.deposit_transactions.get_mut(&tx_detail.tx) {
            if let Some(amount) = resolve_tx_detail.amount {
                if tx_detail.client == resolve_tx_detail.client
                    && resolve_tx_detail.state == TranactionState::Dispute
                    && account.held >= amount
                {
                    //Move the amount from the held back to the available
                    account.held -= amount;
                    account.available += amount;
                    resolve_tx_detail.state = TranactionState::Resolve;
                    return Ok(());
                }
            }
        }
        //resolve disputed withdraw transaction
        else if let Some(resolve_tx_detail) = self.withdrawal_transactions.get_mut(&tx_detail.tx)
        {
            if let Some(amount) = resolve_tx_detail.amount {
                if tx_detail.client == resolve_tx_detail.client
                    && resolve_tx_detail.state == TranactionState::Dispute
                    && account.held >= amount
                {
                    //decrease the held and total
                    account.held -= amount;
                    account.total -= amount;
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
        //chargeback disputed deposit transaction
        if let Some(chargeback_tx_detail) = self.deposit_transactions.get_mut(&tx_detail.tx) {
            if let Some(amount) = chargeback_tx_detail.amount {
                if tx_detail.client == chargeback_tx_detail.client
                    && chargeback_tx_detail.state == TranactionState::Dispute
                    && account.held >= amount
                {
                    //Move the amount from the held back to the available
                    account.held -= amount;
                    account.total -= amount;
                    account.locked = true;
                    chargeback_tx_detail.state = TranactionState::ChargeBack;
                    return Ok(());
                }
            }
        }
        //chargeback disputed withdraw transaction
        else if let Some(chargeback_tx_detail) =
            self.withdrawal_transactions.get_mut(&tx_detail.tx)
        {
            if let Some(amount) = chargeback_tx_detail.amount {
                if tx_detail.client == chargeback_tx_detail.client
                    && chargeback_tx_detail.state == TranactionState::Dispute
                    && account.held >= amount
                {
                    //Move the amount from held back to avaiable
                    account.held -= amount;
                    account.available += amount;
                    account.locked = true;
                    chargeback_tx_detail.state = TranactionState::ChargeBack;
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
            self.process_transaction(transaction);
        }

        self.output();
    }
}

#[cfg(test)]
#[path = "transaction_engine_test.rs"]
mod transaction_engine_test;
