use std::{sync::Arc, time::Instant};
use metrics::histogram;
use tokio_postgres::{Client, Error, NoTls, Statement};


const CREATE_SCHEMA_COMMANDS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS follows (id TEXT NOT NULL, inbox TEXT NOT NULL, actor TEXT NOT NULL, UNIQUE (inbox, actor))",
    "CREATE INDEX IF NOT EXISTS follows_actor ON follows (actor) INCLUDE (inbox)",
];

#[derive(Clone)]
pub struct Database {
    inner: Arc<DatabaseInner>,
}

struct DatabaseInner {
    client: Client,
    add_follow: Statement,
    del_follow: Statement,
    get_following_inboxes: Statement,
    get_follows_count: Statement,
    get_followers_count: Statement,
}

impl Database {
    pub async fn connect(conn_str: &str) -> Self {
        let (client, connection) = tokio_postgres::connect(conn_str, NoTls)
            .await
            .unwrap();

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::error!("postgresql: {}", e);
            }
        });

        for command in CREATE_SCHEMA_COMMANDS {
            client.execute(*command, &[])
                .await
                .unwrap();
        }
        let add_follow = client.prepare("INSERT INTO follows (id, inbox, actor) VALUES ($1, $2, $3)")
            .await
            .unwrap();
        let del_follow = client.prepare("DELETE FROM follows WHERE id=$1 AND actor=$2")
            .await
            .unwrap();
        let get_following_inboxes = client.prepare("SELECT DISTINCT inbox FROM follows WHERE actor=$1")
            .await
            .unwrap();
        let get_follows_count = client.prepare("SELECT COUNT(id) FROM follows")
            .await
            .unwrap();
        let get_followers_count = client.prepare("SELECT COUNT(DISTINCT id) FROM follows")
            .await
            .unwrap();

        Database {
            inner: Arc::new(DatabaseInner {
                client,
                add_follow,
                del_follow,
                get_following_inboxes,
                get_follows_count,
                get_followers_count,
            }),
        }
    }

    pub async fn add_follow(&self, id: &str, inbox: &str, actor: &str) -> Result<(), Error> {
        let t1 = Instant::now();
        self.inner.client.execute(&self.inner.add_follow, &[&id, &inbox, &actor])
            .await?;
        let t2 = Instant::now();
        histogram!("postgres_query_duration", "query" => "add_follow")
            .record(t2 - t1);
        Ok(())
    }

    pub async fn del_follow(&self, id: &str, actor: &str) -> Result<(), Error> {
        let t1 = Instant::now();
        self.inner.client.execute(&self.inner.del_follow, &[&id, &actor])
            .await?;
        let t2 = Instant::now();
        histogram!("postgres_query_duration", "query" => "del_follow")
            .record(t2 - t1);
        Ok(())
    }

    pub async fn get_following_inboxes(&self, actor: &str) -> Result<impl Iterator<Item = String>, Error> {
        let t1 = Instant::now();
        let rows = self.inner.client.query(&self.inner.get_following_inboxes, &[&actor])
            .await?;
        let t2 = Instant::now();
        histogram!("postgres_query_duration", "query" => "get_following_inboxes")
            .record(t2 - t1);
        Ok(rows.into_iter()
           .map(|row| row.get(0))
        )
    }

    pub async fn get_follows_count(&self) -> Result<i64, Error> {
        let row = self.inner.client.query_one(&self.inner.get_follows_count, &[])
            .await?;
        Ok(row.get(0))
    }

    pub async fn get_followers_count(&self) -> Result<i64, Error> {
        let row = self.inner.client.query_one(&self.inner.get_followers_count, &[])
            .await?;
        Ok(row.get(0))
    }
}
