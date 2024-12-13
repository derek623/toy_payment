use serde::de;
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

impl<'de> Deserialize<'de> for Transaction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <Vec<SmolStr>>::deserialize(deserializer)?;
        let r#type = s
            .get(0)
            .ok_or(serde::de::Error::custom("Cannot find type"))?
            .trim()
            .to_lowercase_smolstr();
        tracing::info!("Type is {}", r#type);

        let client: u16 = s
            .get(1)
            .ok_or(serde::de::Error::custom("Cannot find client"))?
            .trim()
            .parse()
            .map_err(de::Error::custom)?;
        tracing::info!("client is {}", client);
        let tx: u32 = s
            .get(2)
            .ok_or(serde::de::Error::custom("Cannot find tx"))?
            .trim()
            .parse()
            .map_err(de::Error::custom)?;
        tracing::info!("tx is {}", tx);
        let amount: Option<f64> = match s.get(3) {
            None => None,
            Some(amount) => Some(amount.trim().parse().map_err(de::Error::custom)?),
        };
        tracing::info!("amount is {:?}", amount);

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
#[derive(Debug, Deserialize)]
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
    pub amount: Option<f64>,
    pub state: TranactionState,
}

impl TransactionDetail {
    pub fn new(client: u16, tx: u32, amount: Option<f64>) -> Self {
        Self {
            client,
            tx,
            amount,
            state: TranactionState::Normal,
        }
    }
}

#[derive(Default)]
pub struct Account {
    pub client: u16,
    pub available: f64,
    pub held: f64,
    pub total: f64,
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
