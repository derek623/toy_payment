# Toy payment engine

------------------------------
INSTRUCTION
------------------------------

As specified in the test document, this program accepts 1 parameter which is the path of the input file. The account summary will be output to stdout and can be redirected to a file like below:

**cargo run -- transactions.csv > accounts.csv**

------------------------------
COMPONENTS
------------------------------

There are 2 main components:

1) Parser, which is responsible for parsing the input csv file, normalizing each entry into an internal format and send them to the transaction engine via a mpsc channel.

2) Transaction engine, which listens for incoming transactions via a mpsc channel, process them and update the accounts accordingly. It has 3 hashmaps, one that store the deposit transactions, one that stores the withdrawal transactions and one that stores the accounts. Once all the transactions are processed, it will output the account summary to stdout. 

Note that the transaction engine is the one that decides if the deserialized transaction is a legitimate transaction (For example, rejecting deposit transaction that doesn't have an amount as amount is an option field in the TransactionDetail struct). I believe the parser is just a parser, it shouldn't have the logic to decide if a specific transaction is formed correctly or not.

------------------------------
LOGS AND ERRORS
------------------------------
All errors are logged in log file, which is generated in the "log" directory. It rolls over every hour.

------------------------------
ASSUMPTIONS
------------------------------
The test specification mentions that for
1) Dispute: held fund increased by the disputed amount and available fund decreased by the disputed amount, total fund unchanged
2) Resolve: held fund decreased by the disputed amount and available fund increased by the disputed amount, total fund unchanged
3) Chargeback: held fund and total fund decreased by the disputed amount

However, it sounds to me the above descriptions only apply to deposit transactions but not withdrawal transactions. For example, a chargeback on a disputed withdrawal transaction should increase the total fund rather than decrease it since it is reversing the original withdrawal transaction. Therefore, I apply the below logics for withdrawal:
1) Dispute: held fund increased by the disputed amount and total fund increased by the disputed amount, available fund unchanged
2) Resolve: held fund decreased by the disputed amount and total fund decreased by the disputed amount, available fund unchanged
3) Chargeback: held fund decreased by the disputed amount and total fund increased by the disputed amount

------------------------------
TESTING
------------------------------
Unit tests have been implemented and sample csv files are generated for testing. I have included a few sample files in the "test_inputs" directory

------------------------------
KNOWN ISSUES/POSSIBLE IMPROVEMENTS
------------------------------
Because we need to handle dispute/resolve/chargeback, the transaction engine needs to store all the deposit and withdrawal transactions. The current implementation stores all the transactions in memory. However, since the transaction id is u32, the total number of records could go up to 4,294,967,296. Each transaction record is 32 bytes, so the total memory needed is around 137 GB, which is a lot. We probably need to implement a better strategy on how to store the transactions. Possible strategies include:
1) Move the least accessed transactions to DB and dig it out when needed
2) Move the oldest transactions to DB and dig it out when needed
