use addok_core::config::AddokConfig;
use addok_core::document::Document;
use addok_core::normalizer::normalize_address;
use addok_core::BanRecord;
use tantivy::schema::*;
use tantivy::TantivyDocument;

#[derive(Clone, Debug)]
pub struct AddokSchema {
    pub schema: Schema,
    pub id: Field,
    pub text: Field,
    pub street: Field,
    pub housenumber: Field,
    pub postcode: Field,
    pub city: Field,
    pub ban_type: Field,
    pub lon: Field,
    pub lat: Field,
    pub payload: Field,
}

impl AddokSchema {
    pub fn new() -> Self {
        let mut builder = Schema::builder();

        // TEXT | STORED indexation
        let text_indexing = TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions);
        let text_options = TextOptions::default()
            .set_indexing_options(text_indexing)
            .set_stored();

        // Exact string fields (postcode, housenumber, id, ban_type)
        let string_options = STRING | STORED;

        let id = builder.add_text_field("id", string_options.clone());
        let text = builder.add_text_field("text", text_options.clone());
        let street = builder.add_text_field("street", text_options.clone());
        let housenumber = builder.add_text_field("housenumber", string_options.clone());
        let postcode = builder.add_text_field("postcode", string_options.clone());
        let city = builder.add_text_field("city", text_options);
        let ban_type = builder.add_text_field("ban_type", string_options);

        // Fast coordinates fields for distance boosting + stored
        let numeric_options = NumericOptions::default().set_fast().set_stored();
        let lon = builder.add_f64_field("lon", numeric_options.clone());
        let lat = builder.add_f64_field("lat", numeric_options);

        // Stored payload field for instant full record retrieval without external storage
        let payload = builder.add_text_field("payload", STORED);

        let schema = builder.build();

        Self {
            schema,
            id,
            text,
            street,
            housenumber,
            postcode,
            city,
            ban_type,
            lon,
            lat,
            payload,
        }
    }

    /// Build a `TantivyDocument` from a `BanRecord` and its normalized text fields.
    pub fn create_document(
        &self,
        record: &BanRecord,
        normalized_text: &str,
        normalized_street: &str,
        normalized_city: &str,
    ) -> TantivyDocument {
        let mut doc = TantivyDocument::default();

        doc.add_text(self.id, &record.id);
        doc.add_text(self.text, normalized_text);
        doc.add_text(self.street, normalized_street);

        if let Some(hn) = record.full_housenumber() {
            doc.add_text(self.housenumber, &hn);
        }

        doc.add_text(self.postcode, record.code_postal.trim());
        doc.add_text(self.city, normalized_city);
        doc.add_text(self.ban_type, record.ban_type().to_string());

        if let Some(lon_val) = record.lon {
            doc.add_f64(self.lon, lon_val);
        }
        if let Some(lat_val) = record.lat {
            doc.add_f64(self.lat, lat_val);
        }

        if let Ok(json_str) = serde_json::to_string(record) {
            doc.add_text(self.payload, &json_str);
        }

        doc
    }

    /// Build a `TantivyDocument` from an Addok `Document`.
    pub fn create_document_from_doc(&self, doc: &Document) -> TantivyDocument {
        self.create_document_from_doc_with_config(doc, &AddokConfig::default())
    }

    /// Build a `TantivyDocument` from an Addok `Document` respecting plugin configuration.
    pub fn create_document_from_doc_with_config(
        &self,
        doc: &Document,
        config: &AddokConfig,
    ) -> TantivyDocument {
        let mut tdoc = TantivyDocument::default();
        tdoc.add_text(self.id, &doc.id);

        let has_france = config.has_france();
        let norm_name = normalize_address(&doc.name, has_france);
        tdoc.add_text(self.street, &norm_name);

        let norm_city = doc
            .city
            .as_deref()
            .map(|c| normalize_address(c, has_france))
            .unwrap_or_default();
        tdoc.add_text(self.city, &norm_city);

        let label = doc.label(None);
        let norm_text = normalize_address(&label, has_france);
        tdoc.add_text(self.text, &norm_text);

        if let Some(ref pc) = doc.postcode {
            tdoc.add_text(self.postcode, pc.trim());
        }

        let address_type = doc.address_type.as_deref().unwrap_or("street");
        tdoc.add_text(self.ban_type, address_type);

        if let Some((lat, lon)) = doc.coordinates(None) {
            tdoc.add_f64(self.lon, lon);
            tdoc.add_f64(self.lat, lat);
        }

        if let Ok(json_str) = serde_json::to_string(doc) {
            tdoc.add_text(self.payload, &json_str);
        }

        tdoc
    }
}

impl Default for AddokSchema {
    fn default() -> Self {
        Self::new()
    }
}
