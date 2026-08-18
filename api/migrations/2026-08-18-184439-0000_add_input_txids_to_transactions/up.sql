-- Parent txids this transaction spends from. NULL means we never managed to
-- learn them, which is distinct from '{}' (a coinbase, which spends nothing).
-- Bootstrap rows carry only the unconfirmed parents `getrawmempool verbose`
-- reports as `depends`; the other write paths carry the full vin set.
ALTER TABLE transactions ADD COLUMN input_txids TEXT[];
