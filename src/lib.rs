// Copyright (c) 2020 rust-mysql-simple contributors
//
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. All files in the project carrying such notice may not be copied,
// modified, or distributed except according to those terms.

//! This create offers:
//!
//! *   MySql database's helper in pure rust;
//! *   A mini orm framework (Just MySQL)。
//!
//! Features:
//!
//! *   Other Database support, i.e. support SQLite, Oracle, MSSQL...;
//! *   support of original SQL;
//! *   support of named parameters for custom condition;
//!
//! ## Installation
//!
//! Put the desired version of the crate into the `dependencies` section of your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! akita = "*"
//! ```
//!
//! ## Annotions.
//! * Table - to make Akita work with structs
//! * column - to make struct field with own database.
//! * name - work with column, make the table's field name. default struct' field name.
//! * exist - ignore struct's field with table. default true.
//!
//! ## Support Field Types.
//! 
//! * ```Option<T>```
//! * ```u8, u32, u64```
//! * ```i32, i64```
//! * ```usize```
//! * ```f32, f64```
//! * ```bool```
//! * ```str, String```
//! * ```NaiveDate, NaiveDateTime```
//! 
//! ## Example
//! 
//! ```rust
//! use akita::prelude::*;
//! 
//! 
//! /// Annotion Support: Table、id、column (name, exist)
//! #[derive(Table, Clone)]
//! #[table(name = "t_system_user")]
//! pub struct User {
//!     #[id(name = "id")]
//!     pub pk: i64,
//!     pub id: String,
//!     pub name: String,
//!     pub headline: NaiveDateTime,
//!     pub avatar_url: Option<String>,
//!     /// 状态
//!     pub status: u8,
//!     /// 用户等级 0.普通会员 1.VIP会员
//!     pub level: u8,
//!     /// 生日
//!     pub birthday: Option<NaiveDate>,
//!     /// 性别
//!     pub gender: u8,
//!     #[column(exist = "false")]
//!     pub is_org: bool,
//!     #[column(name = "token")]
//!     pub url_token: String,
//!     pub data: Vec<String>,
//!     pub user_type: String,
//!     pub inner_struct: TestInnerStruct,
//!     pub inner_tuple: (String),
//!     pub inner_enum: TestInnerEnum,
//! }
//! 
//! impl Default for User {
//!     fn default() -> Self {
//!         Self {
//!             id: "".to_string(),
//!             pk: 0,
//!             name: "".to_string(),
//!             headline: mysql::chrono::Local::now().naive_local(),
//!             avatar_url: "".to_string().into(),
//!             gender: 0,
//!             birthday: mysql::chrono::Local::now().naive_local().date().into(),
//!             is_org: false,
//!             url_token: "".to_string(),
//!             user_type: "".to_string(),
//!             status: 0,
//!             level: 1,
//!             data: vec![],
//!             inner_struct: TestInnerStruct {
//!                 id: "".to_string(),
//!             },
//!             inner_tuple: ("".to_string()),
//!             inner_enum: TestInnerEnum::Field,
//!         }
//!     }
//! }
//! 
//! #[derive(Clone)]
//! pub struct TestInnerStruct {
//!     pub id: String,
//! }
//! 
//! #[derive(Clone)]
//! pub enum TestInnerEnum {
//!     Field,
//! }
//! /// build the wrapper.
//! let mut wrapper = UpdateWrapper::new()
//!     .like(true, "username", "ffff");
//!     .eq(true, "username", 12);
//!     .eq(true, "username", "3333");
//!     .in_(true, "username", vec![1,44,3]);
//!     .not_between(true, "username", 2, 8);
//!     .set(true, "username", 4);
//! 
//! let user = User::default();
//! 
//! // Transaction
//! conn.start_transaction(TxOpts::default()).map(|mut transaction| {
//!     match user.update( & mut wrapper, &mut ConnMut::TxMut(&mut transaction)) {
//!         Ok(res) => {}
//!         Err(err) => {
//!             println!("error : {:?}", err);
//!         }
//!     }
//! });
//!
//! let mut pool = ConnMut::R2d2Polled(conn);
//! /// update by identify
//! match user.update_by_id(&mut conn) {
//!     Ok(res) => {}
//!     Err(err) => {
//!         println!("error : {:?}", err);
//!     }
//! }
//! 
//! /// delete by identify
//! match user.delete_by_id(&mut conn) {
//!     Ok(res) => {}
//!     Err(err) => {
//!         println!("error : {:?}", err);
//!     }
//! }
//! 
//! /// delete by condition
//! match user.delete:: < UpdateWrapper > ( & mut wrapper, &mut conn) {
//!     Ok(res) => {}
//!     Err(err) => {
//!         println!("error : {:?}", err);
//!     }
//! }
//! 
//! /// insert data
//! match user.insert(&mut conn) {
//!     Ok(res) => {}
//!     Err(err) => {
//!         println!("error : {:?}", err);
//!     }
//! }
//! 
//! /// find by identify
//! match user.find_by_id(&mut conn) {
//!     Ok(res) => {}
//!     Err(err) => {
//!         println!("error : {:?}", err);
//!     }
//! }
//! 
//! 
//! /// find one by condition
//! match user.find_one::<UpdateWrapper>(&mut wrapper, &mut conn) {
//!     Ok(res) => {}
//!     Err(err) => {
//!         println!("error : {:?}", err);
//!     }
//! }
//! 
//! /// find page by condition
//! match user.page::<UpdateWrapper>(1, 10,&mut wrapper, &mut conn) {
//!     Ok(res) => {}
//!     Err(err) => {
//!         println!("error : {:?}", err);
//!     }
//! }
//! 
//! ```
//! ## API Documentation
//! ## Wrapper
//! ```ignore
//! 
//! let mut wrapper = UpdateWrapper::new();
//! wrapper.like(true, "column1", "ffff");
//! wrapper.eq(true, "column2", 12);
//! wrapper.eq(true, "column3", "3333");
//! wrapper.in_(true, "column4", vec![1,44,3]);
//! wrapper.not_between(true, "column5", 2, 8);
//! wrapper.set(true, "column1", 4);
//! match wrapper.get_target_sql("t_user") {
//!     Ok(sql) => {println!("ok:{}", sql);}
//!     Err(err) => {println!("err:{}", err);}
//! }
//! ```
//! ```
//! Update At 2021.07.13 10:21 
//! By Mr.Pan
//! 
//! 
//! 
#[allow(unused)]
mod comm;
mod wrapper;
mod segment;
mod errors;
mod mapper;
mod mysql;

#[doc(inline)]
pub use wrapper::{QueryWrapper, UpdateWrapper, Wrapper};
#[doc(inline)]
pub use mapper::{BaseMapper, IPage, ConnMut};
#[doc(inline)]
pub use segment::SqlSegment;
#[doc(inline)]
pub use errors::AkitaError;
#[doc(inline)]
pub use crate::mysql::{FromRowExt, from_long_row, new_pool};
#[cfg(feature = "r2d2_pool")]
pub use crate::mysql::{R2d2Pool, PooledConn};

pub use chrono;

pub mod prelude {
    #[doc(inline)]
    pub use mysql::{params, prelude::Queryable};
    pub use chrono::{Local, NaiveDate, NaiveDateTime};
    #[doc(inline)]
    pub use mysql::{Conn, Opts, OptsBuilder};
}

// Re-export #[derive(Table)].
//
// The reason re-exporting is not enabled by default is that disabling it would
// be annoying for crates that provide handwritten impls or data formats. They
// would need to disable default features and then explicitly re-enable std.
#[cfg(feature = "akita_derive")]
#[allow(unused_imports)]
#[macro_use]
extern crate akita_derive;
#[cfg(feature = "akita_derive")]
#[doc(hidden)]
pub use akita_derive::*;