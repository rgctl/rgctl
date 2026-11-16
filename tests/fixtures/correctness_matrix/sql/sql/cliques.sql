-- Community separation cliques for SQL correctness matrix (UTF-8: α β)

CREATE FUNCTION clique_alpha_c()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT 10;
$$;

CREATE FUNCTION clique_alpha_b()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT clique_alpha_c();
$$;

CREATE FUNCTION clique_alpha_a()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT clique_alpha_b() + clique_alpha_c();
$$;

CREATE FUNCTION clique_beta_c()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT 20;
$$;

CREATE FUNCTION clique_beta_b()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT clique_beta_c();
$$;

CREATE FUNCTION clique_beta_a()
RETURNS INT
LANGUAGE sql
AS $$
    SELECT clique_beta_b() + clique_beta_c();
$$;
