pub async fn find_in_path(binary_name: &str) -> Option<String> {
    let which_cmd = if cfg!(windows) { "where" } else { "which" };
    let output = tokio::process::Command::new(which_cmd)
        .arg(binary_name)
        .output()
        .await
        .ok()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()?
            .trim()
            .to_string();
        if !path.is_empty() && tokio::fs::metadata(&path).await.is_ok() {
            return Some(path);
        }
    }
    None
}
