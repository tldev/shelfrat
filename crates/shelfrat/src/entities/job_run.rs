use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "job_runs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub job_name: String,
    pub status: String,
    pub started_at: DateTime,
    pub finished_at: Option<DateTime>,
    pub result: Option<String>,
    pub triggered_by: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
