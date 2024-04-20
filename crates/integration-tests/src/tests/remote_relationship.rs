use crate::query;
use insta::assert_yaml_snapshot;
use serde_json::json;

#[tokio::test]
async fn provides_source_and_target_for_remote_relationship() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        query(
            r#"
                query AlbumMovies($limit: Int, $movies_limit: Int) {
                  album(limit: $limit, order_by: { title: Asc }) {
                    title
                    movies(limit: $movies_limit) {
                      title
                      runtime
                    }
                    albumId
                  }
                }
            "#
        )
        .variables(json!({ "limit": 11, "movies_limit": 2 }))
        .run()
        .await?
    );
    Ok(())
}
