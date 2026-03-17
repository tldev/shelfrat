use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "books")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub file_path: String,
    pub file_hash: String,
    pub file_format: String,
    pub file_size_bytes: i64,
    pub added_at: DateTime,
    pub last_seen_at: DateTime,
    pub missing: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_one = "super::book_metadata::Entity")]
    Metadata,
    #[sea_orm(has_many = "super::book_author::Entity")]
    Authors,
    #[sea_orm(has_many = "super::book_tag::Entity")]
    Tags,
}

impl Related<super::book_metadata::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Metadata.def()
    }
}

impl Related<super::author::Entity> for Entity {
    fn to() -> RelationDef {
        super::book_author::Relation::Author.def()
    }
    fn via() -> Option<RelationDef> {
        Some(super::book_author::Relation::Book.def().rev())
    }
}

impl Related<super::tag::Entity> for Entity {
    fn to() -> RelationDef {
        super::book_tag::Relation::Tag.def()
    }
    fn via() -> Option<RelationDef> {
        Some(super::book_tag::Relation::Book.def().rev())
    }
}

impl ActiveModelBehavior for ActiveModel {}
