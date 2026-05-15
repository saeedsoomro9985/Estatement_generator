/// MongoDB connection settings for the EStatements database.
#[derive(Debug, Clone)]
pub struct MongoConfig {
    pub uri: String,
    pub database: String,
    pub collection: String,
}

impl Default for MongoConfig {
    fn default() -> Self {
        Self {
            uri: "mongodb://localhost:27017".to_string(),
            database: "EStatements".to_string(),
            collection: "Statements".to_string(),
        }
    }
}

impl MongoConfig {
    pub fn new(uri: impl Into<String>, database: impl Into<String>, collection: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            database: database.into(),
            collection: collection.into(),
        }
    }
}
