# Payment Pipeline

Handles chargeback reconciliation and settlement verification (UTF-8: € £ ¥).

## Chargeback Reconciliation

Reconcile chargeback ledger entries against bank settlement feeds.

```sql
SELECT id, amount FROM chargeback_ledger;
```
