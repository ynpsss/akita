//! 
//! Fuse features
//!

use std::convert::TryFrom;
use std::fmt::{Debug, Formatter};
use akita_core::{FieldType, GetTableName, Table};
use once_cell::sync::OnceCell;

use crate::segment::ISegment;
use crate::{AkitaError, AkitaMapper, IPage, Pool, Wrapper, database::DatabasePlatform, AkitaConfig};
use crate::{cfg_if, Params, TableName, DatabaseName, SchemaContent, TableDef, Rows, FromValue, Value, ToValue, GetFields};
use crate::database::Platform;
use crate::manager::{AkitaTransaction, build_insert_clause, build_update_clause};
use crate::pool::{PlatformPool, PooledConnection};

cfg_if! {if #[cfg(feature = "akita-mysql")]{
    use crate::platform::{mysql::{self, MysqlDatabase}};
}}

cfg_if! {if #[cfg(feature = "akita-sqlite")]{
    use crate::platform::sqlite::{self, SqliteDatabase};
}}


pub struct Akita{
    /// the connection pool
    pool: OnceCell<PlatformPool>,
    cfg: AkitaConfig,
}

pub enum AkitaType {
    Query,
    Update,
}

impl Akita {
    
    pub fn new(cfg: AkitaConfig) -> Result<Self, AkitaError> {
        let pool = Self::init_database(&cfg)?;
        let platform = Self::init_pool(&cfg)?;
        Ok(Self {
            pool: OnceCell::from(platform),
            cfg
        })
    }

    pub fn from_pool(pool: &Pool) -> Result<Self, AkitaError> {
        let platform = pool.get_pool()?;
        Ok(Self {
            pool: OnceCell::from(platform.clone()),
            cfg: pool.config().clone()
        })
    }


    /// get a database instance with a connection, ready to send sql statements
    fn init_database(cfg: &AkitaConfig) -> Result<DatabasePlatform, AkitaError> {
        let database_url = cfg.url();
        let platform: Result<Platform, _> = TryFrom::try_from(database_url.as_str());
        match platform {
            Ok(platform) => match platform {
                #[cfg(feature = "akita-mysql")]
                Platform::Mysql => {
                    let pool_mysql = mysql::init_pool(&cfg)?;
                    let pooled_conn = pool_mysql.get()?;
                    Ok(DatabasePlatform::Mysql(Box::new(MysqlDatabase::new(pooled_conn, cfg.to_owned()))))
                }
                #[cfg(feature = "akita-sqlite")]
                Platform::Sqlite(path) => {
                    cfg.set_url(path);
                    let pool_sqlite = sqlite::init_pool(&cfg)?;
                    let pooled_conn = pool_sqlite.get()?;
                    Ok(DatabasePlatform::Sqlite(Box::new(SqliteDatabase::new(pooled_conn, cfg.to_owned()))))
                }
                Platform::Unsupported(scheme) => Err(AkitaError::UnknownDatabase(scheme))
            },
            Err(e) => Err(AkitaError::UrlParseError(e.to_string())),
        }
    }

    /// get a database instance with a connection, ready to send sql statements
    fn init_pool(cfg: &AkitaConfig) -> Result<PlatformPool, AkitaError> {
        let database_url = cfg.url();
        let platform: Result<Platform, _> = TryFrom::try_from(database_url.as_str());
        match platform {
            Ok(platform) => match platform {
                #[cfg(feature = "akita-mysql")]
                Platform::Mysql => {
                    let pool_mysql = mysql::init_pool(&cfg)?;
                    Ok(PlatformPool::MysqlPool(pool_mysql))
                }
                #[cfg(feature = "akita-sqlite")]
                Platform::Sqlite(path) => {
                    cfg.set_url(path);
                    let pool_sqlite = sqlite::init_pool(&cfg)?;
                    Ok(PlatformPool::SqlitePool(pool_sqlite))
                }
                Platform::Unsupported(scheme) => Err(AkitaError::UnknownDatabase(scheme))
            },
            Err(e) => Err(AkitaError::UrlParseError(e.to_string())),
        }
    }

    pub fn start_transaction(&self) -> Result<AkitaTransaction, AkitaError> {
        let mut conn = self.acquire()?;
        conn.start_transaction()?;
        Ok(AkitaTransaction {
            conn: &self,
            committed: false,
            rolled_back: false,
        })
    }

    /// get conn pool
    pub fn get_pool(&self) -> Result<&PlatformPool, AkitaError> {
        let p = self.pool.get();
        if p.is_none() {
            return Err(AkitaError::R2D2Error("[akita] akita pool not inited!".to_string()));
        }
        return Ok(p.unwrap());
    }

    /// get an DataBase Connection used for the next step
    pub fn acquire(&self) -> Result<DatabasePlatform, AkitaError> {
        let pool = self.get_pool()?;
        let conn = pool.acquire()?;
        match conn {
            #[cfg(feature = "akita-mysql")]
            PooledConnection::PooledMysql(pooled_mysql) => Ok(DatabasePlatform::Mysql(Box::new(MysqlDatabase::new(*pooled_mysql, self.cfg.to_owned())))),
            #[cfg(feature = "akita-sqlite")]
            PooledConnection::PooledSqlite(pooled_sqlite) => Ok(DatabasePlatform::Sqlite(Box::new(SqliteDatabase::new(*pooled_sqlite, self.cfg.to_owned())))),
        }
    }

    pub fn new_wrapper(&self) -> Wrapper {
        Wrapper::new()
    }

    pub fn affected_rows(&self) -> u64 {
        let conn = self.acquire().expect("cannot get db pool");
        conn.affected_rows()
    }

    pub fn last_insert_id(&self) -> u64 {
        let conn = self.acquire().expect("cannot get db pool");
        conn.last_insert_id()
    }


    /// called multiple times when using database platform that doesn;t support multiple value
    pub fn save_map<T>(&self, table: &str, entity: &T) -> Result<(), AkitaError>
    where
        T: ToValue,
    {
        let columns = entity.to_value();
        let columns = if let Some(columns) = columns.as_object() {
            columns.keys().collect::<Vec<&String>>()
        } else { Vec::new() };
        let sql = self.build_insert_clause_map(table,&[entity])?;
        let data = entity.to_value();
        let mut values: Vec<Value> = Vec::with_capacity(columns.len());
        for col in columns.iter() {
            let value = data.get_obj_value(col);
            match value {
                Some(value) => values.push(value.clone()),
                None => values.push(Value::Nil),
            }
        }
        let _bvalues: Vec<&Value> = values.iter().collect();
        let mut conn = self.acquire()?;
        conn.execute_result(&sql,values.into())?;
        Ok(())
    }

    /// called multiple times when using database platform that doesn;t support multiple value
    pub fn save_map_batch<T>(&self,table: &str, entities: &[&T]) -> Result<(), AkitaError>
        where
            T: ToValue,
    {
        if entities.len() == 0 {
            return Err(AkitaError::DataError("data cannot be empty".to_string()))
        }
        let columns = entities[0].to_value();
        let columns = if let Some(columns) = columns.as_object() {
            columns.keys().collect::<Vec<&String>>()
        } else { Vec::new() };
        let sql = self.build_insert_clause_map(table, entities)?;
        let mut values: Vec<Value> = Vec::with_capacity(columns.len());
        for entity in entities.iter() {
           for col in columns.iter() {
               let data = entity.to_value();
               let value = data.get_obj_value(col);
               match value {
                   Some(value) => values.push(value.clone()),
                   None => values.push(Value::Nil),
               }
           }
        }
        let _bvalues: Vec<&Value> = values.iter().collect();
        let mut conn = self.acquire()?;
        conn.execute_result(&sql,values.into())?;
        Ok(())
    }

    /// build an update clause
    pub fn build_update_clause(&self, table: &str, mut wrapper: Wrapper) -> Result<String, AkitaError> {
        let wrapper = &mut wrapper.clone();
        let mut sql = String::new();
        sql += &format!("update {} ", table);
        let platform = self.acquire()?;
        let fields = wrapper.fields_set.iter().map(|f| f.0.to_owned()).collect::<Vec<String>>();
            // columns.iter().filter(|col| !set_fields.is_empty() && fields.contains(&col.name) && col.exist).collect::<Vec<_>>()
            sql += &format!(
                "set {}",
                wrapper.fields_set
                    .iter_mut()
                    .enumerate()
                    .map(|(x, (col, value))| {
                        #[allow(unreachable_patterns)]
                        match platform {
                            #[cfg(feature = "akita-mysql")]
                            DatabasePlatform::Mysql(_) => format!("`{}` = {}", col, value.get_sql_segment()),
                            #[cfg(feature = "akita-sqlite")]
                            DatabasePlatform::Sqlite(_) => format!("`{}` = ${}", col, x + 1),
                            _ => format!("`{}` = ${}", col, x + 1),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        let where_condition = wrapper.get_sql_segment();
        if !where_condition.is_empty() {
            sql += &format!(" where {} ", where_condition);
        }
        Ok(sql)
    }

    /// build an insert clause
    pub fn build_insert_clause_map<T>(&self, table: &str, entities: &[T]) -> Result<String, AkitaError>
    where
        T: ToValue,
    {
        let platform = self.acquire()?;
        if entities.len() == 0 {
            return Err(AkitaError::DataError("data cannot be empty".to_string()))
        }
        let columns = entities[0].to_value();
        let columns = if let Some(columns) = columns.as_object() {
            columns.keys().collect::<Vec<&String>>()
        } else { Vec::new() };
        let columns_len = columns.len();
        let mut sql = String::new();
        sql += &format!("INSERT INTO {} ", table);
        sql += &format!(
            "({})\n",
            columns
                .iter()
                .map(|c| format!("`{}`", c))
                .collect::<Vec<_>>()
                .join(", ")
        );
        sql += "VALUES ";
        sql += &entities
            .iter()
            .enumerate()
            .map(|(y, _)| {
                format!(
                    "\n\t({})",
                    columns
                        .iter()
                        .enumerate()
                        .map(|(x, _)| {
                            #[allow(unreachable_patterns)]
                            match platform {
                                #[cfg(feature = "with-sqlite")]
                                DatabasePlatform::Sqlite(_) => format!("${}", y * columns_len + x + 1),
                                #[cfg(feature = "akita-mysql")]
                                DatabasePlatform::Mysql(_) => "?".to_string(),
                                _ => format!("${}", y * columns_len + x + 1),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        Ok(sql)
    }
}

impl AkitaMapper for Akita {
    /// Get all the table of records
    fn list<T>(&self, mut wrapper:Wrapper) -> Result<Vec<T>, AkitaError>
        where
            T: GetTableName + GetFields + FromValue,

    {
        let table = T::table_name();
        if table.complete_name().is_empty() {
            return Err(AkitaError::MissingTable("Find Error, Missing Table Name !".to_string()))
        }
        let columns = T::fields();
        let enumerated_columns = columns
            .iter().filter(|f| f.exist)
            .map(|c| format!("`{}`", c.name))
            .collect::<Vec<_>>()
            .join(", ");
        let select_fields = wrapper.get_select_sql();
        let enumerated_columns = if select_fields.eq("*") {
            enumerated_columns
        } else {
            select_fields
        };
        let where_condition = wrapper.get_sql_segment();
        let where_condition = if where_condition.trim().is_empty() { String::default() } else { format!("WHERE {}",where_condition) };
        let sql = format!("SELECT {} FROM {} {}", &enumerated_columns, &table.complete_name(),where_condition);
        let mut conn = self.acquire()?;
        let rows = conn.execute_result(&sql, Params::Nil)?;
        let mut entities = vec![];
        for data in rows.iter() {
            let entity = T::from_value(&data);
            entities.push(entity)
        }
        Ok(entities)
    }

    /// Get one the table of records
    fn select_one<T>(&self, mut wrapper:Wrapper) -> Result<Option<T>, AkitaError>
        where
            T: GetTableName + GetFields + FromValue,
    {
        let table = T::table_name();
        if table.complete_name().is_empty() {
            return Err(AkitaError::MissingTable("Find Error, Missing Table Name !".to_string()))
        }
        let columns = T::fields();
        let enumerated_columns = columns
            .iter().filter(|f| f.exist)
            .map(|c| format!("`{}`", c.name))
            .collect::<Vec<_>>()
            .join(", ");
        let select_fields = wrapper.get_select_sql();
        let enumerated_columns = if select_fields.eq("*") {
            enumerated_columns
        } else {
            select_fields
        };
        let where_condition = wrapper.get_sql_segment();
        let where_condition = if where_condition.trim().is_empty() { String::default() } else { format!("WHERE {}",where_condition) };
        let sql = format!("SELECT {} FROM {} {}", &enumerated_columns, &table.complete_name(), where_condition);
        let mut conn = self.acquire()?;
        let rows = conn.execute_result(&sql, Params::Nil)?;
        Ok(rows.iter().next().map(|data| T::from_value(&data)))
    }

    /// Get one the table of records by id
    fn select_by_id<T, I>(&self, id: I) -> Result<Option<T>, AkitaError>
        where
            T: GetTableName + GetFields + FromValue,
            I: ToValue
    {
        let table = T::table_name();
        if table.complete_name().is_empty() {
            return Err(AkitaError::MissingTable("Find Error, Missing Table Name !".to_string()))
        }
        let columns = T::fields();
        let col_len = columns.len();
        let enumerated_columns = columns
            .iter().filter(|f| f.exist)
            .map(|c| format!("`{}`", c.name))
            .collect::<Vec<_>>()
            .join(", ");
        let mut conn = self.acquire()?;
        if let Some(field) = columns.iter().find(| field| match field.field_type {
            FieldType::TableId(_) => true,
            FieldType::TableField => false,
        }) {
            let sql = match conn {
                #[cfg(feature = "akita-mysql")]
                DatabasePlatform::Mysql(_) => format!("SELECT {} FROM {} WHERE `{}` = ? limit 1", &enumerated_columns, &table.complete_name(), &field.name),
                #[cfg(feature = "akita-sqlite")]
                DatabasePlatform::Sqlite(_) => format!("SELECT {} FROM {} WHERE `{}` = ${} limit 1", &enumerated_columns, &table.complete_name(), &field.name, col_len + 1),
                _ => format!("SELECT {} FROM {} WHERE `{}` = ${} limit 1", &enumerated_columns, &table.complete_name(), &field.name, col_len + 1),
            };

            let rows = conn.execute_result(&sql, (id.to_value(),).into())?;
            Ok(rows.iter().next().map(|data| T::from_value(&data)))
        } else {
            Err(AkitaError::MissingIdent(format!("Table({}) Missing Ident...", &table.name)))
        }
    }

    /// Get table of records with page
    fn page<T>(&self, page: usize, size: usize, mut wrapper:Wrapper) -> Result<IPage<T>, AkitaError>
        where
            T: GetTableName + GetFields + FromValue,

    {
        let table = T::table_name();
        if table.complete_name().is_empty() {
            return Err(AkitaError::MissingTable("Find Error, Missing Table Name !".to_string()))
        }
        let columns = T::fields();
        let enumerated_columns = columns
            .iter().filter(|f| f.exist)
            .map(|c| format!("`{}`", c.name))
            .collect::<Vec<_>>()
            .join(", ");
        let select_fields = wrapper.get_select_sql();
        let enumerated_columns = if select_fields.eq("*") {
            enumerated_columns
        } else {
            select_fields
        };
        let where_condition = wrapper.get_sql_segment();
        let where_condition = if where_condition.trim().is_empty() { String::default() } else { format!("WHERE {}",where_condition) };
        let count_sql = format!("select count(1) as count from {} {}", &table.complete_name(), where_condition);
        let count: i64 = self.exec_first(&count_sql, ())?;
        let mut page = IPage::new(page, size ,count as usize, vec![]);
        if page.total > 0 {
            let sql = format!("SELECT {} FROM {} {} limit {}, {}", &enumerated_columns, &table.complete_name(), where_condition,page.offset(),  page.size);
            let mut conn = self.acquire()?;
            let rows = conn.execute_result(&sql, Params::Nil)?;
            let mut entities = vec![];
            for dao in rows.iter() {
                let entity = T::from_value(&dao);
                entities.push(entity)
            }
            page.records = entities;
        }
        Ok(page)
    }

    /// Get the total count of records
    fn count<T>(&self, mut wrapper:Wrapper) -> Result<usize, AkitaError>
        where
            T: GetTableName + GetFields,
    {
        let table = T::table_name();
        if table.complete_name().is_empty() {
            return Err(AkitaError::MissingTable("Find Error, Missing Table Name !".to_string()))
        }
        let where_condition = wrapper.get_sql_segment();
        let where_condition = if where_condition.trim().is_empty() { String::default() } else { format!("WHERE {}",where_condition) };
        let sql = format!(
            "SELECT COUNT(1) AS count FROM {} {}",
            table.complete_name(),
            where_condition
        );
        self.exec_first(&sql, ())
    }

    /// Remove the records by wrapper.
    fn remove<T>(&self, mut wrapper:Wrapper) -> Result<u64, AkitaError>
        where
            T: GetTableName + GetFields,
    {
        let table = T::table_name();
        if table.complete_name().is_empty() {
            return Err(AkitaError::MissingTable("Find Error, Missing Table Name !".to_string()))
        }
        let where_condition = wrapper.get_sql_segment();
        let where_condition = if where_condition.trim().is_empty() { String::default() } else { format!("WHERE {}",where_condition) };
        let sql = format!("delete from {} {}", &table.complete_name(), where_condition);
        let mut conn = self.acquire()?;
        let rows = conn.execute_result(&sql, Params::Nil)?;
        Ok(conn.affected_rows())
    }

    /// Remove the records by id.
    fn remove_by_id<T, I>(&self, id: I) -> Result<u64, AkitaError>
        where
            I: ToValue,
            T: GetTableName + GetFields {
        let table = T::table_name();
        if table.complete_name().is_empty() {
            return Err(AkitaError::MissingTable("Find Error, Missing Table Name !".to_string()))
        }
        let cols = T::fields();
        let mut conn = self.acquire()?;
        let col_len = cols.len();
        if let Some(field) = cols.iter().find(| field| match field.field_type {
            FieldType::TableId(_) => true,
            FieldType::TableField => false,
        }) {
            let sql = match conn {
                #[cfg(feature = "akita-mysql")]
                DatabasePlatform::Mysql(_) => format!("delete from {} where `{}` = ?", &table.name, &field.name),
                #[cfg(feature = "akita-sqlite")]
                DatabasePlatform::Sqlite(_) => format!("delete from {} where `{}` = ${}", &table.name, &field.name, col_len + 1),
                _ => format!("delete from {} where `{}` = ${}", &table.name, &field.name, col_len + 1),
            };
            let rows = conn.execute_result(&sql, (id.to_value(),).into())?;
            Ok(conn.affected_rows())
        } else {
            Err(AkitaError::MissingIdent(format!("Table({}) Missing Ident...", &table.name)))
        }
    }


    /// Remove the records by wrapper.
    fn remove_by_ids<T, I>(&self, ids: Vec<I>) -> Result<u64, AkitaError>
        where
            I: ToValue,
            T: GetTableName + GetFields {
        let table = T::table_name();
        if table.complete_name().is_empty() {
            return Err(AkitaError::MissingTable("Find Error, Missing Table Name !".to_string()))
        }
        let cols = T::fields();
        let mut conn = self.acquire()?;
        let col_len = cols.len();
        if let Some(field) = cols.iter().find(| field| match field.field_type {
            FieldType::TableId(_) => true,
            FieldType::TableField => false,
        }) {
            let sql = match conn {
                #[cfg(feature = "akita-mysql")]
                DatabasePlatform::Mysql(_) => format!("delete from {} where `{}` in (?)", &table.name, &field.name),
                #[cfg(feature = "akita-sqlite")]
                DatabasePlatform::Sqlite(_) => format!("delete from {} where `{}` in (${})", &table.name, &field.name, col_len + 1),
                _ => format!("delete from {} where `{}` = ${}", &table.name, &field.name, col_len + 1),
            };
            let ids = ids.iter().map(|v| v.to_value().to_string()).collect::<Vec<String>>().join(",");
            let rows = conn.execute_result(&sql, (ids,).into())?;
            Ok(conn.affected_rows())
        } else {
            Err(AkitaError::MissingIdent(format!("Table({}) Missing Ident...", &table.name)))
        }
    }


    /// Update the records by wrapper.
    fn update<T>(&self, entity: &T, mut wrapper: Wrapper) -> Result<u64, AkitaError>
        where
            T: GetTableName + GetFields + ToValue {
        let table = T::table_name();
        if table.complete_name().is_empty() {
            return Err(AkitaError::MissingTable("Find Error, Missing Table Name !".to_string()))
        }
        let mut conn = self.acquire()?;
        let columns = T::fields();
        let sql = build_update_clause(&conn, entity, &mut wrapper);
        let update_fields = wrapper.fields_set;
        let mut bvalues: Vec<&Value> = Vec::new();
        if update_fields.is_empty() {
            let data = entity.to_value();
            let mut values: Vec<Value> = Vec::with_capacity(columns.len());
            for col in columns.iter() {
                if !col.exist || col.field_type.ne(&FieldType::TableField) {
                    continue;
                }
                let col_name = &col.name;
                let mut value = data.get_obj_value(&col_name);
                match &col.fill {
                    None => {}
                    Some(v) => {
                        match v.mode.as_ref() {
                            "update" | "default" => {
                                value = v.value.as_ref();
                            }
                            _=> {}
                        }
                    }
                }
                match value {
                    Some(value) => values.push(value.clone()),
                    None => values.push(Value::Nil),
                }
            }

            let rows = conn.execute_result(&sql, values.into())?;
        } else {
            let rows = conn.execute_result(&sql, Params::Nil)?;
        }
        Ok(conn.affected_rows())
    }

    /// Update the records by id.
    fn update_by_id<T>(&self, entity: &T) -> Result<u64, AkitaError>
        where
            T: GetTableName + GetFields + ToValue {
        let table = T::table_name();
        if table.complete_name().is_empty() {
            return Err(AkitaError::MissingTable("Find Error, Missing Table Name !".to_string()))
        }
        let data = entity.to_value();
        let columns = T::fields();
        let col_len = columns.len();
        let mut conn = self.acquire()?;
        if let Some(field) = T::fields().iter().find(| field| match field.field_type {
            FieldType::TableId(_) => true,
            FieldType::TableField => false,
        }) {
            let set_fields = columns
                .iter().filter(|col| col.exist && col.field_type == FieldType::TableField)
                .enumerate()
                .map(|(x, col)| {
                    #[allow(unreachable_patterns)]
                    match conn {
                        #[cfg(feature = "akita-mysql")]
                        DatabasePlatform::Mysql(_) => format!("`{}` = ?", &col.name),
                        #[cfg(feature = "akita-sqlite")]
                        DatabasePlatform::Sqlite(_) => format!("`{}` = ${}",&col.name, x + 1),
                        _ => format!("`{}` = ${}", &col.name, x + 1),
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let sql = match conn {
                #[cfg(feature = "akita-mysql")]
                DatabasePlatform::Mysql(_) => format!("update {} set {} where `{}` = ?", &table.name, &set_fields, &field.name),
                #[cfg(feature = "akita-sqlite")]
                DatabasePlatform::Sqlite(_) => format!("update {} set {} where `{}` = ${}", &table.name, &set_fields, &field.name, col_len + 1),
                _ => format!("update {} set {} where `{}` = ${}", &table.name, &set_fields, &field.name, col_len + 1),
            };
            let mut values: Vec<Value> = Vec::with_capacity(columns.len());
            let id = data.get_obj_value(&field.name);
            for col in columns.iter() {
                if !col.exist || col.field_type.ne(&FieldType::TableField) {
                    continue;
                }
                let col_name = &col.name;
                let mut value = data.get_obj_value(col_name);
                match &col.fill {
                    None => {}
                    Some(v) => {
                        match v.mode.as_ref() {
                            "update" | "default" => {
                                value = v.value.as_ref();
                            }
                            _=> {}
                        }
                    }
                }
                match value {
                    Some(value) => values.push(value.clone()),
                    None => values.push(Value::Nil),
                }
            }
            match id {
                Some(id) => values.push(id.clone()),
                None => {
                    return Err(AkitaError::MissingIdent(format!("Table({}) Missing Ident value...", &table.name)));
                }
            }
            let _ = conn.execute_result(&sql, values.into())?;
            Ok(conn.affected_rows())
        } else {
            Err(AkitaError::MissingIdent(format!("Table({}) Missing Ident...", &table.name)))
        }

    }

    #[allow(unused_variables)]
    fn save_batch<T>(&self, entities: &[&T]) -> Result<(), AkitaError>
        where
            T: GetTableName + GetFields + ToValue
    {
        let columns = T::fields();
        let mut conn = self.acquire()?;
        let sql = build_insert_clause(&conn, entities);

        let mut values: Vec<Value> = Vec::with_capacity(entities.len() * columns.len());
        for entity in entities.iter() {
            for col in columns.iter() {
                let data = entity.to_value();
                let mut value = data.get_obj_value(&col.name);
                match &col.fill {
                    None => {}
                    Some(v) => {
                        match v.mode.as_ref() {
                            "insert" | "default" => {
                                value = v.value.as_ref();
                            }
                            _ => {}
                        }
                    }
                }
                match value {
                    Some(value) => values.push(value.clone()),
                    None => values.push(Value::Nil),
                }
            }
        }
        let bvalues: Vec<&Value> = values.iter().collect();
        conn.execute_result(&sql,values.into())?;
        Ok(())
    }

    /// called multiple times when using database platform that doesn;t support multiple value
    fn save<T, I>(&self, entity: &T) -> Result<Option<I>, AkitaError>
        where
            T: GetTableName + GetFields + ToValue,
            I: FromValue,
    {
        let columns = T::fields();
        let mut conn = self.acquire()?;
        let sql = build_insert_clause(&conn, &[entity]);
        let data = entity.to_value();
        let mut values: Vec<Value> = Vec::with_capacity(columns.len());
        for col in columns.iter() {
            let mut value = data.get_obj_value(&col.name);
            match &col.fill {
                None => {}
                Some(v) => {
                    match v.mode.as_ref() {
                        "insert" | "default" => {
                            value = v.value.as_ref();
                        }
                        _=> {}
                    }
                }
            }
            match value {
                Some(value) => values.push(value.clone()),
                None => values.push(Value::Nil),
            }
        }
        let _bvalues: Vec<&Value> = values.iter().collect();

        conn.execute_result(&sql,values.into())?;
        let rows: Rows = match conn {
            #[cfg(feature = "akita-mysql")]
            DatabasePlatform::Mysql(_) => conn.execute_result("SELECT LAST_INSERT_ID();", Params::Nil)?,
            #[cfg(feature = "akita-sqlite")]
            DatabasePlatform::Sqlite(_) => conn.execute_result("SELECT LAST_INSERT_ROWID();", Params::Nil)?,
        };
        let last_insert_id = rows.iter().next().map(|data| I::from_value(&data));
        Ok(last_insert_id)
    }

    /// save or update
    fn save_or_update<T, I>(&self, entity: &T) -> Result<Option<I>, AkitaError>
        where
            T: GetTableName + GetFields + ToValue,
            I: FromValue {
        let data = entity.to_value();
        let id = if let Some(field) = T::fields().iter().find(| field| match field.field_type {
            FieldType::TableId(_) => true,
            FieldType::TableField => false,
        }) {
            data.get_obj_value(&field.name).unwrap_or(&Value::Nil)
        } else { &Value::Nil };
        match id {
            Value::Nil => {
                self.save(entity)
            },
            _ => {
                self.update_by_id(entity)?;
                Ok(I::from_value(id).into())
            }
        }
    }

    fn exec_iter<S: Into<String>, P: Into<Params>>(&self, sql: S, params: P) -> Result<Rows, AkitaError> {
        let mut conn = self.acquire()?;
        let rows = conn.execute_result(&sql.into(), params.into())?;
        Ok(rows)
    }
}

mod test {
    use std::time::Duration;
    use akita_core::ToValue;
    use once_cell::sync::Lazy;
    use crate::{Akita, AkitaTable, self as akita, AkitaConfig, LogLevel, AkitaMapper};

    pub static AK:Lazy<Akita> = Lazy::new(|| {
        let mut cfg = AkitaConfig::new("mysql://longchen:Zhengtayigeyi@1@139.196.111.46:3306/dc_pay".to_string());
        cfg = cfg.set_max_size(5).set_connection_timeout(Duration::from_secs(5)).set_log_level(LogLevel::Info);
        let mut akita = Akita::new(cfg).unwrap();
        akita
    });
    #[derive(Clone, Debug, AkitaTable)]
    pub struct MchInfo {
        #[table_id]
        pub mch_no: Option<String>,
        #[field(fill( function = "fffff", mode = "default"))]
        pub mch_name: Option<String>,
    }

    #[sql(AK,"select * from mch_info where mch_no = ?")]
    fn select(name: &str) -> Vec<MchInfo> {
        todo!()
    }

    fn fffff() -> String {
        println!("跑起来啦");
        String::from("test")

    }

    #[test]
    #[cfg(feature = "akita-mysql")]
    fn test_akita() {
        let mut cfg = AkitaConfig::new("xxxxx".to_string());
        cfg = cfg.set_max_size(5).set_connection_timeout(Duration::from_secs(5)).set_log_level(LogLevel::Info);
        let mut akita = Akita::new(cfg).unwrap();
        let wrapper = akita.new_wrapper();
        // let data = akita.select_by_id::<MchInfo, _>("23234234").unwrap();
        let s = select("23234234");
        println!("ssssssss{:?}",data);
        // let s = select("i");
    }
}