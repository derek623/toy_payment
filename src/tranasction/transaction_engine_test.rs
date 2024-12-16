#[cfg(test)]
mod tests {
    use crate::models::Transaction::{ChargeBack, Deposit, Dispute, Resolve, Withdrawal};
    use crate::models::{TranactionState, TransactionDetail};
    use crate::TransactionEngine;
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
        locked: bool,
    ) {
        let account = engine.accounts.get(&account_id).unwrap();
        assert_approx_eq!(account.available.to_f64().unwrap(), available);
        assert_approx_eq!(account.total.to_f64().unwrap(), total);
        assert_approx_eq!(account.held.to_f64().unwrap(), held);
        assert_eq!(account.locked, locked);
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
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 1, 0, false);

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
        check_account(&engine, 1, 3.0, 0_f64, 3.0, 2, 0, false);

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
        check_account(&engine, 1, 1.95, 0_f64, 1.95, 2, 1, false);

        //an invalid withdraw with dup transaction id
        let tx = TransactionDetail::new(1, 4, Some(dec!(1.95)));
        assert_eq!(
            format!("{}", engine.process_withdrawal(tx).unwrap_err()),
            "Duplicate transaction id 4"
        );
        check_account(&engine, 1, 1.95, 0_f64, 1.95, 2, 1, false);

        //Withdraw more than available
        let tx = TransactionDetail::new(1, 5, Some(dec!(1.96)));
        assert_eq!(
            format!("{}", engine.process_withdrawal(tx).unwrap_err()),
            "Withdraw error for tx 5"
        );
        check_account(&engine, 1, 1.95, 0_f64, 1.95, 2, 1, false);

        //Withdraw everything
        let tx = TransactionDetail::new(1, 5, Some(dec!(1.95)));
        let _ = engine.process_withdrawal(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 0_f64, 0_f64, 0_f64, 2, 2, false);
    }

    #[test]
    fn test_multiple_account_withdraw_() {
        let mut engine = get_transaction_engine();
        //a deposit for client 1
        let tx = Deposit(TransactionDetail::new(1, 1, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 1, 0, false);

        //a deposit for client 2
        let tx = Deposit(TransactionDetail::new(2, 2, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0, false);

        //a deposit for client 3
        let tx = Deposit(TransactionDetail::new(3, 3, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 3);
        check_account(&engine, 3, 1.1111, 0_f64, 1.1111, 3, 0, false);

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
        check_account(&engine, 3, 0_f64, 0_f64, 0_f64, 3, 1, false);

        //a withdraw for client 2
        let tx = Withdrawal(TransactionDetail::new(2, 6, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 4);
        check_account(&engine, 2, 0_f64, 0_f64, 0_f64, 3, 2, false);

        //a withdraw for client 1
        let tx = Withdrawal(TransactionDetail::new(1, 7, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 4);
        check_account(&engine, 1, 0_f64, 0_f64, 0_f64, 3, 3, false);
    }

    #[test]
    fn test_deposit_dispute_resolve() {
        let mut engine = get_transaction_engine();
        //a deposit for client 1
        let tx = Deposit(TransactionDetail::new(1, 1, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 1, 0, false);

        //a deposit for client 2
        let tx = Deposit(TransactionDetail::new(2, 2, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0, false);

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
        check_account(&engine, 1, 0_f64, 1.1111, 1.1111, 2, 0, false);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0, false);
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
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 2, 0, false);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0, false);
        check_transaction(&engine, 1, TranactionState::Resolve);

        //invalid resolve, transaction is already resolved
        let tx = TransactionDetail::new(1, 1, None);
        assert_eq!(
            format!("{}", engine.process_resolve(tx).unwrap_err()),
            "Resolve error for tx 1"
        );
    }

    #[test]
    fn test_withdraw_dispute_resolve() {
        let mut engine = get_transaction_engine();
        //a deposit for client 1
        let tx = Deposit(TransactionDetail::new(1, 1, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 1, 0, false);

        //a deposit for client 2
        let tx = Deposit(TransactionDetail::new(2, 2, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0, false);

        //a withdraw for client 1
        let tx = Withdrawal(TransactionDetail::new(1, 3, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 1, 0_f64, 0_f64, 0_f64, 2, 1, false);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 1, false);

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
        check_account(&engine, 1, 0_f64, 1.1111, 1.1111, 2, 1, false);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 1, false);
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
        check_account(&engine, 1, 0_f64, 0_f64, 0_f64, 2, 1, false);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 1, false);
        check_transaction(&engine, 3, TranactionState::Resolve);

        //Invalid resolve, incorrect state
        let tx = TransactionDetail::new(1, 3, None);
        assert_eq!(
            format!("{}", engine.process_resolve(tx).unwrap_err()),
            "Resolve error for tx 3"
        );
    }

    #[test]
    fn test_deposit_dispute_chargeback() {
        let mut engine = get_transaction_engine();
        //a deposit for client 1
        let tx = Deposit(TransactionDetail::new(1, 1, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 1, 0, false);

        //a deposit for client 2
        let tx = Deposit(TransactionDetail::new(2, 2, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0, false);

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
        check_account(&engine, 1, 0_f64, 1.1111, 1.1111, 2, 0, false);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0, false);
        check_transaction(&engine, 1, TranactionState::Dispute);

        //invalid chargeback as transaction doesn't exist
        let tx = TransactionDetail::new(1, 3, None);
        assert_eq!(
            format!("{}", engine.process_chargeback(tx).unwrap_err()),
            "Chargeback error for tx 3"
        );

        //invalid chargeback as client is incorrect
        let tx = TransactionDetail::new(2, 1, None);
        assert_eq!(
            format!("{}", engine.process_chargeback(tx).unwrap_err()),
            "Chargeback error for tx 1"
        );

        //invalid chargeback as transaction is not in dispute state
        let tx = TransactionDetail::new(2, 2, None);
        assert_eq!(
            format!("{}", engine.process_chargeback(tx).unwrap_err()),
            "Chargeback error for tx 2"
        );

        //valid chargeback for client 1
        let tx = ChargeBack(TransactionDetail::new(1, 1, None));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 1, 0_f64, 0_f64, 0_f64, 2, 0, true);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0, false);
        check_transaction(&engine, 1, TranactionState::ChargeBack);

        //invalid chargeback as account is locked
        let tx = TransactionDetail::new(1, 1, None);
        assert_eq!(
            format!("{}", engine.process_chargeback(tx).unwrap_err()),
            "Account 1 is locked"
        );
    }

    #[test]
    fn test_withdraw_dispute_chargeback() {
        let mut engine = get_transaction_engine();
        //a deposit for client 1
        let tx = Deposit(TransactionDetail::new(1, 1, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 1);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 1, 0, false);

        //a deposit for client 2
        let tx = Deposit(TransactionDetail::new(2, 2, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 0, false);

        //a withdraw for client 1
        let tx = Withdrawal(TransactionDetail::new(1, 3, Some(dec!(1.1111))));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 1, 0_f64, 0_f64, 0_f64, 2, 1, false);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 1, false);

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
        check_account(&engine, 1, 0_f64, 1.1111, 1.1111, 2, 1, false);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 1, false);
        check_transaction(&engine, 3, TranactionState::Dispute);

        //invalid chargeback as transaction doesn't exist
        let tx = TransactionDetail::new(1, 4, None);
        assert_eq!(
            format!("{}", engine.process_chargeback(tx).unwrap_err()),
            "Chargeback error for tx 4"
        );

        //invalid chargeback as client is incorrect
        let tx = TransactionDetail::new(2, 3, None);
        assert_eq!(
            format!("{}", engine.process_chargeback(tx).unwrap_err()),
            "Chargeback error for tx 3"
        );

        //valid chargeback for client 1
        let tx = ChargeBack(TransactionDetail::new(1, 3, None));
        let _ = engine.process_transaction(tx);
        assert_eq!(engine.accounts.len(), 2);
        check_account(&engine, 1, 1.1111, 0_f64, 1.1111, 2, 1, true);
        check_account(&engine, 2, 1.1111, 0_f64, 1.1111, 2, 1, false);
        check_transaction(&engine, 3, TranactionState::ChargeBack);

        //invalid chargeback as account is locked
        let tx = TransactionDetail::new(1, 3, None);
        assert_eq!(
            format!("{}", engine.process_chargeback(tx).unwrap_err()),
            "Account 1 is locked"
        );
    }
}
