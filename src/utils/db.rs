use anyhow::Context;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

pub async fn make_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let options = connect_options(database_url)?;
    PgPoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await
        .context("connecting to PostgreSQL")
}

pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!()
        .run(pool)
        .await
        .context("running migrations")
}

fn connect_options(database_url: &str) -> anyhow::Result<PgConnectOptions> {
    let trimmed = database_url.trim();
    if trimmed.starts_with("postgres://") || trimmed.starts_with("postgresql://") {
        return trimmed.parse().context("parsing DATABASE_URL");
    }

    let mut options = PgConnectOptions::new();
    for pair in trimmed.split_whitespace() {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        options = match key {
            "host" => options.host(value),
            "hostaddr" => options.host(value),
            "port" => options.port(value.parse().context("parsing DATABASE_URL port")?),
            "dbname" | "database" => options.database(value),
            "user" => options.username(value),
            "password" => options.password(value),
            _ => options,
        };
    }
    Ok(options)
}

#[macro_export]
macro_rules! query_scale {
    ($pool:expr, $sql:expr) => {{
        sqlx::query_scalar($sql)
            .fetch_one($pool)
            .await?
    }};

    ($pool:expr, $sql:expr, $($bind:expr),+ $(,)?) => {{
        let mut query = sqlx::query_scalar($sql);
        $(
            query = query.bind($bind);
        )+
        query.fetch_one($pool).await?
    }};
}

#[macro_export]
macro_rules! query_list {
    ($pool:expr, $model:ty, $table:expr) => {
        sqlx::query_as::<_, $model>(&format!("SELECT * FROM {}", $table))
            .fetch_all($pool)
            .await?
    };
    ($pool:expr, $model:ty, $table:expr, $where:expr $(, $arg:expr)*) => {
        sqlx::query_as::<_, $model>(&format!("SELECT * FROM {} WHERE {}", $table, $where))
            $(.bind($arg))*
            .fetch_all($pool)
            .await?
    };
}

#[macro_export]
macro_rules! query_get {
    ($pool:expr, $model:ty, $table:expr, $id_col:expr, $id_val:expr) => {
        sqlx::query_as::<_, $model>(&format!("SELECT * FROM {} WHERE {} = $1", $table, $id_col))
            .bind($id_val)
            .fetch_optional($pool)
            .await?
    };
}

#[macro_export]
macro_rules! query_delete {
    ($pool:expr, $table:expr, $id_col:expr, $id_val:expr) => {
        sqlx::query(&format!("DELETE FROM {} WHERE {} = $1", $table, $id_col))
            .bind($id_val)
            .execute($pool)
            .await?
    };
}

#[macro_export]
macro_rules! query_create {
    ($pool:expr, $model:ty, $table:expr, $($col:expr => $val:expr),+ $(,)?) => {{
        let cols: Vec<&str> = vec![$($col),+];
        let placeholders: Vec<String> = (1..=cols.len())
            .map(|i| format!("${}", i))
            .collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) RETURNING *",
            $table,
            cols.join(", "),
            placeholders.join(", ")
        );
        let mut query = sqlx::query_as::<_, $model>(&sql);
        $(
            query = query.bind($val);
        )+
        query.fetch_one($pool).await?
    }};

    ($pool:expr, $table:expr, $($col:expr => $val:expr),+ $(,)?) => {{
        let cols: Vec<&str> = vec![$($col),+];
        let placeholders: Vec<String> = (1..=cols.len())
            .map(|i| format!("${}", i))
            .collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) RETURNING *",
            $table,
            cols.join(", "),
            placeholders.join(", ")
        );
        let mut query = sqlx::query(&sql);
        $(
            query = query.bind($val);
        )+
        query.fetch_one($pool).await?
    }};
}

#[macro_export]
macro_rules! query_update {
    ($pool:expr, $model:ty, $table:expr, $id_col:expr, $id_val:expr, $($col:expr => $val:expr),+ $(,)?) => {{
        let mut set_clauses: Vec<String> = Vec::new();
        let mut param_index = 1usize;

        $(
            if $val.is_some() {
                set_clauses.push(format!("{} = ${}", $col, param_index));
                param_index += 1;
            }
        )+

        let sql = format!(
            "UPDATE {} SET {} WHERE {} = ${} RETURNING *",
            $table,
            set_clauses.join(", "),
            $id_col,
            param_index
        );

        let mut query = sqlx::query_as::<_, $model>(&sql);

        $(
            if let Some(v) = $val {
                query = query.bind(v);
            }
        )+

        query.bind($id_val).fetch_one($pool).await?
    }};
}
