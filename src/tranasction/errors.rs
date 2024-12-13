use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransactionErrors {
    #[error("Deposit error for tx {0}")]
    DepositError(DepositError),
    #[error("Withdraw error for tx {0}")]
    WithdrawalError(WithdrawalError),
}

#[derive(Debug)]
pub struct DepositError {
    pub tx: u32,
}

impl fmt::Display for DepositError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.tx)
    }
}

#[derive(Debug)]
pub struct WithdrawalError {
    pub tx: u32,
}

impl fmt::Display for WithdrawalError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.tx)
    }
}
