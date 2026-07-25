pub type Id = String;

pub fn generate_id() -> Id {
    Uuid::new_v4().to_string()
}
