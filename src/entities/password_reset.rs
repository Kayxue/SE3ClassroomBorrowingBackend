use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "password_reset")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,

    #[sea_orm(unique)]
    pub email: String,

    pub code: Option<String>,
    pub code_expires_at: Option<DateTimeWithTimeZone>,

    pub reset_token: Option<String>,
    pub token_expires_at: Option<DateTimeWithTimeZone>,

    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}