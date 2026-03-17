use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "authors")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub name: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::book_author::Entity")]
    BookAuthors,
}

impl Related<super::book::Entity> for Entity {
    fn to() -> RelationDef {
        super::book_author::Relation::Book.def()
    }
    fn via() -> Option<RelationDef> {
        Some(super::book_author::Relation::Author.def().rev())
    }
}

impl ActiveModelBehavior for ActiveModel {}
