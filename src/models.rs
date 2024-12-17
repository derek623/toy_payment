use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{de, Serialize};
use serde::{Deserialize, Deserializer};
use smol_str::{SmolStr, StrExt};

//Type of the transactions
#[derive(Debug)]
pub enum Transaction {
    Deposit(TransactionDetail),
    Withdrawal(TransactionDetail),
    Dispute(TransactionDetail),
    Resolve(TransactionDetail),
    ChargeBack(TransactionDetail),
    Unknown,
}

//customer deserailizer to deserialzie each entry into the Transaction enum
impl<'de> Deserialize<'de> for Transaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <Vec<SmolStr>>::deserialize(deserializer)?;
        let r#type = s
            .first()
            .ok_or(serde::de::Error::custom("Cannot find type"))?
            .to_lowercase_smolstr();
        let client: u16 = s
            .get(1)
            .ok_or(serde::de::Error::custom("Cannot find client"))?
            .parse()
            .map_err(de::Error::custom)?;
        let tx: u32 = s
            .get(2)
            .ok_or(serde::de::Error::custom("Cannot find tx"))?
            .parse()
            .map_err(de::Error::custom)?;
        //round to 4 decimal places
        let amount: Option<Decimal> = match s.get(3) {
            Some(amount) if !amount.is_empty() => Some(
                Decimal::from_str(amount)
                    .map_err(de::Error::custom)?
                    .round_dp(4),
            ),
            _ => None,
        };

        let t = TransactionDetail::new(client, tx, amount);
        Ok(match r#type.as_str() {
            "deposit" => Transaction::Deposit(t),
            "withdrawal" => Transaction::Withdrawal(t),
            "dispute" => Transaction::Dispute(t),
            "resolve" => Transaction::Resolve(t),
            "chargeback" => Transaction::ChargeBack(t),
            _ => Transaction::Unknown,
        })
    }
}

//State of the transaction. Normal is either Deposit or Withdrawl that do not have any dispute
#[derive(Debug, Deserialize, PartialEq, PartialOrd)]
pub enum TranactionState {
    Normal,
    Dispute,
    Resolve,
    ChargeBack,
}

//Detail of the transaction
#[derive(Debug, Deserialize)]
pub struct TransactionDetail {
    pub client: u16,
    pub tx: u32,
    pub amount: Option<Decimal>,
    pub state: TranactionState,
}

impl TransactionDetail {
    pub fn new(client: u16, tx: u32, amount: Option<Decimal>) -> Self {
        Self {
            client,
            tx,
            amount,
            state: TranactionState::Normal,
        }
    }
}

#[derive(Default, Clone, Serialize, Debug)]
pub struct Account {
    pub client: u16,
    pub available: Decimal,
    pub held: Decimal,
    pub total: Decimal,
    pub locked: bool,
}

impl Account {
    pub fn new(client: u16) -> Self {
        Self {
            client,
            ..Default::default()
        }
    }
}
