-- AST coverage corpus: exercise every active SQL handler kind.

CREATE SCHEMA myschema;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE DATABASE appdb;
CREATE ROLE app_user;
CREATE SEQUENCE user_id_seq;
CREATE TYPE mood AS ENUM ('sad', 'ok', 'happy');
CREATE TYPE address AS (street TEXT, city TEXT);

CREATE TABLE users (
  id INT PRIMARY KEY,
  name VARCHAR(100) NOT NULL,
  tags INT[],
  scores INT ARRAY,
  age BIGINT DEFAULT 0,
  score DECIMAL(10, 2),
  active BOOLEAN,
  created_at TIMESTAMP
);

CREATE INDEX idx_users_name ON users (name);
CREATE MATERIALIZED VIEW mv_users AS SELECT id, name FROM users;
CREATE OR REPLACE VIEW v_users AS
  WITH cte AS (
    SELECT u.id, u.name, COUNT(*) AS c
    FROM users u
    LEFT JOIN users u2 ON u.id = u2.id
    CROSS JOIN users u3
    WHERE u.age BETWEEN 1 AND 100
      AND u.name IS NOT NULL
      AND NOT (u.age < 0)
    GROUP BY u.id, u.name
    HAVING COUNT(*) > 0
    WINDOW w AS (
      PARTITION BY u.id
      ORDER BY u.name
      ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
    )
  )
  SELECT
    id,
    name,
    c,
    CASE
      WHEN c > 1 THEN 'multi'
      ELSE 'single'
    END AS kind,
    ROW_NUMBER() OVER (PARTITION BY id ORDER BY name) AS rn,
    SUM(c) FILTER (WHERE c > 0) OVER w AS filtered,
    SUM(c) OVER (
      PARTITION BY id
      ORDER BY name
      ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING
    ) AS framed
  FROM cte
  ORDER BY name
  LIMIT 10 OFFSET 0;

CREATE FUNCTION foo(x INT) RETURNS INT
LANGUAGE sql
AS $$
DECLARE
  y INT := 0;
BEGIN
  RETURN x + 1;
END;
$$;

CREATE TRIGGER trg_users
AFTER INSERT ON users
FOR EACH ROW
EXECUTE FUNCTION foo();

ALTER TABLE users ADD COLUMN email TEXT;
ALTER TABLE users ALTER COLUMN name SET NOT NULL;
ALTER TABLE users ADD CONSTRAINT users_email_unique UNIQUE (email);
ALTER TABLE users DROP CONSTRAINT users_email_unique;
ALTER TABLE users DROP COLUMN email;
ALTER VIEW v_users RENAME TO v_users_renamed;
ALTER TYPE mood ADD VALUE 'meh';
ALTER SCHEMA myschema RENAME TO myschema2;

INSERT INTO users (id, name) VALUES (1, 'a'), (2, 'b');
UPDATE users SET name = 'c' WHERE id = 1;
DELETE FROM users WHERE id = 2;

MERGE INTO users u
USING (SELECT 3 AS id, 'd' AS name) s
ON u.id = s.id
WHEN MATCHED THEN UPDATE SET name = s.name
WHEN NOT MATCHED THEN INSERT (id, name) VALUES (s.id, s.name);

SELECT * FROM users WHERE id IN (1, 2);
SELECT (1 + 2) * 3, -age FROM users;
SELECT a.id FROM users a
  LEFT JOIN LATERAL (SELECT 1 AS x) b ON true;
SELECT a.id FROM users a
  CROSS JOIN LATERAL (SELECT 1 AS x) b;

SELECT id FROM users
UNION
SELECT id FROM users;

COMMENT ON TABLE users IS 'people';
SET search_path TO public;
RESET search_path;
BEGIN TRANSACTION;
  SELECT 1;
COMMIT TRANSACTION;

DROP TRIGGER IF EXISTS trg_users ON users;
DROP FUNCTION foo;
DROP MATERIALIZED VIEW mv_users;
DROP VIEW v_users_renamed;
DROP INDEX idx_users_name;
DROP TABLE users;
DROP TYPE mood;
DROP TYPE address;
DROP SEQUENCE user_id_seq;
DROP ROLE app_user;
DROP SCHEMA myschema2;
DROP EXTENSION pgcrypto;
DROP DATABASE appdb;
