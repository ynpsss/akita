//! 
//! Generate Database Methods.
//! 
use proc_macro::TokenStream;
use proc_macro_error::{ proc_macro_error};
use syn::{parse_macro_input, AttributeArgs, ItemFn};

#[macro_use]
mod table_derive;
#[macro_use]
mod convert_derive;
mod sql_derive;
#[allow(unused)]
mod util;
mod comm;


/// Generate table info data
#[proc_macro_derive(FromValue)]
pub fn from_akita(input: TokenStream) -> TokenStream {
    convert_derive::impl_from_akita(input)
}

/// Format table info data
#[proc_macro_derive(ToValue)]
pub fn to_akita(input: TokenStream) -> TokenStream {
    convert_derive::impl_to_akita(input)
}

/// Generate table info
/// ```rust
/// /// Annotion Support: Table、table_id、field (name, exist)
/// #[derive(Debug, FromValue, ToValue, AkitaTable, Clone)]
/// #[table(name="t_system_user")]
/// struct SystemUser {
///     #[field = "name"]
///     id: Option<i32>,
///     #[table_id]
///     username: String,
///     #[field(name="ages", exist = "false")]
///     age: i32,
/// }
/// ```
/// 
#[proc_macro_derive(AkitaTable, attributes(field, table, table_id, fill))]
#[proc_macro_error]
pub fn to_table(input: TokenStream) -> TokenStream {
    table_derive::impl_get_table(input)
}

/// auto create sql macro,this macro use RB.fetch_prepare and RB.exec_prepare
/// <pre>
/// for example:
///     pub static AK:Lazy<Akita> = Lazy::new(|| {
///         let mut cfg = AkitaConfig::new("xxxx".to_string()).set_max_size(5).set_connection_timeout(Duration::from_secs(5)).set_log_level(LogLevel::Info);
///         kita::new(cfg).unwrap()
///     });
///     #[sql(AK,"select * from mch_info where mch_no = ?")]
///     fn select(name: &str) -> Vec<MchInfo> { todo!() }
///
/// or:
///    #[sql(AK,"select * from mch_info where mch_no = ?")]
///     fn select(ak: &AKita, name: &str) -> Vec<MchInfo> { todo!() }
/// </pre>
#[proc_macro_attribute]
pub fn sql(args: TokenStream, func: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as AttributeArgs);
    let target_fn: ItemFn = syn::parse(func).unwrap();
    let stream = sql_derive::impl_sql(&target_fn, &args);
    stream
}