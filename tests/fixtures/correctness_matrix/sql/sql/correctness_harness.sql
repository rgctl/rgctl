-- Correctness harness for SQL (UTF-8: λ → Ω)

CREATE TABLE chargeback_ledger (
    id INT PRIMARY KEY,
    amount INT NOT NULL
);

CREATE FUNCTION correctness_leaf()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT 42;
$$;

CREATE FUNCTION correctness_mid()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT CASE WHEN 1 > 0 THEN correctness_leaf() ELSE 0 END;
$$;

CREATE FUNCTION correctness_root()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT CASE WHEN 1 > 0 THEN correctness_mid() ELSE 0 END;
$$;

CREATE FUNCTION correctness_shared()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT 7;
$$;

CREATE FUNCTION correctness_left()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT correctness_shared();
$$;

CREATE FUNCTION correctness_right()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT correctness_shared();
$$;

CREATE FUNCTION archive_chargeback_audit()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT count(*) FROM chargeback_ledger;
$$;

CREATE FUNCTION reconcile_chargeback_ledger()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT archive_chargeback_audit();
$$;

CREATE FUNCTION v2_clone_exact_a()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT CASE WHEN 10 + 20 > 15 THEN 30 ELSE 0 END;
$$;

CREATE FUNCTION v2_clone_exact_b()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT CASE WHEN 10 + 20 > 15 THEN 30 ELSE 0 END;
$$;
