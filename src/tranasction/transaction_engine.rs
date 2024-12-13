use super::errors::{DepositError, TransactionErrors, WithdrawalError};
use crate::models::{Account, Transaction, TransactionDetail};
use ahash::AHashMap;
use anyhow::bail;
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
        //ignore unknown transaction

        match tx {
            Transaction::Unknown => {
                tracing::error!("Skipped unknown transaction");
                return;
            }
            Transaction::Deposit(tx_detail) => {
                let tx_id = tx_detail.tx;

                if let Err(e) = self.process_deposit(&tx_detail) {
                    tracing::error!("Fail to deposit: {e:?}");
                }

                self.deposit_transactions.insert(tx_id, tx_detail);
            }
            Transaction::Withdrawal(tx_detail) => {
                let tx_id = tx_detail.tx;

                if let Err(e) = self.process_withdrawal(&tx_detail) {
                    tracing::error!("Fail to withdraw: {e:?}");
                }

                self.withdrawal_transactions.insert(tx_id, tx_detail);
            }
            Transaction::Dispute(tx_detail) => {
                if let Err(e) = self.process_dispute(&tx_detail) {
                    tracing::error!("Fail to withdraw: {e:?}");
                }
            }
            Transaction::Resolve(tx_detail) => {
                if let Err(e) = self.process_resolve(&tx_detail) {
                    tracing::error!("Fail to withdraw: {e:?}");
                }
            }
            Transaction::ChargeBack(tx_detail) => {
                if let Err(e) = self.process_chargeback(&tx_detail) {
                    tracing::error!("Fail to withdraw: {e:?}");
                }
            }
            _ => {}
        }
    }

    fn process_deposit(&mut self, tx_detail: &TransactionDetail) -> anyhow::Result<()> {
        let account = self
            .accounts
            .entry(tx_detail.client)
            .or_insert(Account::new(tx_detail.client));
        if let Some(amount) = tx_detail.amount {
            if amount > 0_f64 {
                account.available += amount;
                account.total += amount;
                return Ok(());
            }
        }
        bail!(TransactionErrors::DepositError(DepositError {
            tx: tx_detail.tx
        },))
        /*Err(anyhow::Error::new(TransactionErrors::DepositError(
            DepositError,
        )))*/
    }

    fn process_withdrawal(&mut self, tx_detail: &TransactionDetail) -> anyhow::Result<()> {
        let account = self
            .accounts
            .entry(tx_detail.client)
            .or_insert(Account::new(tx_detail.client));
        if let Some(amount) = tx_detail.amount {
            if amount > 0_f64 && account.available >= amount {
                account.available -= amount;
                account.total -= amount;
                return Ok(());
            }
        }
        bail!(TransactionErrors::WithdrawalError(WithdrawalError {
            tx: tx_detail.tx
        },))
    }

    //The doc mentioned that during a dispute, the held fund is increased by the dispute amount and the available fund is decreased by. I assume that
    //this is referring to a dispute for a withdrawal transaction as it simply means moving fund from the the available fund to the held fund. For disputing a
    // withdrawal, I don't think we should decrease the avaiable fund as the client as disputing an incorrect amount being debit from his/her account. So for the dispute
    //of a withdrawal transaction, I decided to increment the held fund only, which means the total fund will increase. However, since the client can't really use that amount yet,
    //so I believe it's fine.
    fn process_dispute(&mut self, tx_detail: &TransactionDetail) -> anyhow::Result<()> {
        let account = self
            .accounts
            .entry(tx_detail.client)
            .or_insert(Account::new(tx_detail.client));
        Ok(())
    }

    fn process_resolve(&mut self, tx_detail: &TransactionDetail) -> anyhow::Result<()> {
        Ok(())
    }

    fn process_chargeback(&mut self, tx_detail: &TransactionDetail) -> anyhow::Result<()> {
        Ok(())
    }

    fn output(&self) {}

    pub async fn run(&mut self) {
        while let Some(transaction) = self.rx.recv().await {
            tracing::info!("Got {:?}", transaction);
            self.process_transaction(transaction);
        }

        self.output();
    }
}
