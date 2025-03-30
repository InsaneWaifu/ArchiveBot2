use async_sqlite::PoolBuilder;

pub type DatabasePool = async_sqlite::Pool;

pub async fn create_database_pool() -> DatabasePool {
    PoolBuilder::new()
        .path("archivebot2.db")
        .journal_mode(async_sqlite::JournalMode::Wal)
        .open().await.unwrap()
}

#[derive(Clone)]
pub enum Query {

}

static QUERIES: phf::Map<Query, ()> = phf::phf_map! {

};