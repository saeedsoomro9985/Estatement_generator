// Run: mongosh "mongodb://localhost:27017/EStatements" --file sql/mongo-index.js
db.Statements.createIndex(
  { "customer.cif": 1, "generatedAt": -1 },
  { name: "ix_customer_cif_generatedAt", background: true }
);
