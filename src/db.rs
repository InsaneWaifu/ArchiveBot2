use std::time::{SystemTime, UNIX_EPOCH};

use deadpool_diesel::{Manager, Pool};
use diesel::prelude::*;

use crate::schema;

#[derive(Queryable, Selectable, Clone)]
#[diesel(table_name = crate::schema::objects)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Object {
    pub id: i32,
    pub path: String,
    pub name: String,
    pub size: i64,
    pub expiry_unix: i64,
    pub user: i64,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::objects)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(belongs_to(User))]
pub struct NewObject {
    pub path: String,
    pub name: String,
    pub size: i64,
    pub expiry_unix: i64,
    pub user: i64,
}

impl NewObject {
    pub fn new_with_extension(ext: &str) -> NewObject {
        let tmpfile = tempfile::NamedTempFile::with_suffix(".".to_owned() + ext).unwrap();
        let tmpfile = tmpfile.into_temp_path().keep().unwrap();
        NewObject {
            path: tmpfile.to_string_lossy().to_owned().to_string(),
            name: "".to_owned(),
            size: 0,
            expiry_unix: 0,
            user: 0,
        }
    }
}

#[derive(Queryable, Selectable, Insertable, Clone)]
#[diesel(table_name = crate::schema::users)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct User {
    pub snowflake: i64,
    pub name_cached: Option<String>,
}

impl User {
    pub fn get_or_create(
        user_snowflake: i64,
        username: String,
        conn: &mut SqliteConnection,
    ) -> anyhow::Result<Self> {
        let name = Some(username);
        use diesel::prelude::*;
        use schema::users::dsl::{name_cached, users};
        let user = users
            .find(user_snowflake)
            .select(User::as_select())
            .first(conn)
            .optional()?;
        if let Some(user) = user {
            if user.name_cached != name {
                diesel::update(users.find(user_snowflake))
                    .set(name_cached.eq(name))
                    .returning(User::as_returning())
                    .get_result(conn)?;
            }
            Ok(user)
        } else {
            let user = User {
                snowflake: user_snowflake,
                name_cached: name,
            };
            diesel::insert_into(crate::schema::users::table)
                .values(&[user.clone()])
                .execute(conn)?;
            Ok(user)
        }
    }

    pub fn get(user_snowflake: i64, db: &mut SqliteConnection) -> anyhow::Result<Self> {
        use crate::schema::users::dsl::*;
        let user = users
            .filter(snowflake.eq(user_snowflake))
            .first::<User>(db)?;
        Ok(user)
    }
}

#[derive(Queryable, Selectable, Insertable, Clone)]
#[diesel(table_name = crate::schema::sharex_config)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(belongs_to(User))]
pub struct SharexConfig {
    pub user_id: i64,
    pub json: String,
}

pub type DatabasePool = Pool<Manager<SqliteConnection>>;

pub async fn create_database_pool() -> DatabasePool {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");
    let manager = deadpool_diesel::Manager::new(database_url, deadpool::Runtime::Tokio1);
    let pool = Pool::builder(manager).max_size(8).build().unwrap();
    pool
}


pub fn delete_expired_files(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    let unix_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    use crate::schema::objects::dsl::*;
    diesel::delete(objects.filter(expiry_unix.lt(unix_time))).execute(conn)?;
    Ok(())
}