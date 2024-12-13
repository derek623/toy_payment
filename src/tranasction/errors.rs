use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransactionErrors {
    #[error("Deposit error for tx {0}")]
    Deposit(DepositError),
    #[error("Withdraw error for tx {0}")]
    Withdrawal(WithdrawalError),
    #[error("Withdraw error for tx {0}")]
    Dispute(DisputeError),
    #[error("Withdraw error for tx {0}")]
    Resolve(ResolveError),
    #[error("Withdraw error for tx {0}")]
    Chargeback(ChargebackError),
    #[error("Account {0} is locked")]
    AccountLock(AccountLockError),
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

#[derive(Debug)]
pub struct DisputeError {
    pub tx: u32,
}

impl fmt::Display for DisputeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.tx)
    }
}

#[derive(Debug)]
pub struct ResolveError {
    pub tx: u32,
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.tx)
    }
}

#[derive(Debug)]
pub struct ChargebackError {
    pub tx: u32,
}

impl fmt::Display for ChargebackError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.tx)
    }
}

#[derive(Debug)]
pub struct AccountLockError {
    pub client: u16,
}

impl fmt::Display for AccountLockError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.client)
    }
}
