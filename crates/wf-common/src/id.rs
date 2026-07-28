use uuid::Uuid;

pub fn generate_id() -> String {
    Uuid::now_v7().to_string()
}
