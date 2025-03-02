use std::collections::HashSet;

use axum::{
    debug_handler,
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{
    routes::data::{set_deserialize, Completion, CompletionPair},
    AppState,
};

pub fn size_deserialize<'de, D>(deserializer: D) -> Result<Option<HashSet<(u32, u32)>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        s.split(',')
            .filter_map(|range| {
                let parts: Vec<_> = range.split('-').collect();
                if parts.is_empty() && parts.len() != 2 {
                    return None;
                }
                let Some(from) = parts.first() else {
                return None;
            };
                let Some(to) = parts.last() else {
                return None;
            };
                let Ok(from) = from.parse::<u32>() else {
                return None;
            };
                let Ok(to) = to.parse::<u32>() else {
                return None;
            };
                if from > to {
                    return None;
                }
                Some((from, to))
            })
            .collect(),
    ))
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    TimeAsc,
    #[default]
    TimeDesc,
    NameAsc,
    NameDesc,
    RecentAsc,
    RecentDesc,
}

impl Sort {
    pub fn get_order_by(&self) -> (&'static str, &'static str) {
        match self {
            Sort::TimeAsc => (
                r#"
                LEFT JOIN (
                  SELECT
                    DISTINCT ON (transactions.args ->> 'account_id') transactions.args ->> 'account_id' as account_id,
                    transactions.timestamp
                  FROM
                    transactions
                  WHERE
                    transactions.method_name = 'add_project'
                  ORDER BY
                    transactions.args ->> 'account_id' ASC,
                    transactions.timestamp ASC
                ) as txs ON projects.id = txs.account_id
                "#,
                "ORDER BY txs.timestamp ASC",
            ),
            Sort::TimeDesc => (
                r#"
                LEFT JOIN (
                  SELECT
                    DISTINCT ON (transactions.args ->> 'account_id') transactions.args ->> 'account_id' as account_id,
                    transactions.timestamp
                  FROM
                    transactions
                  WHERE
                    transactions.method_name = 'add_project'
                  ORDER BY
                    transactions.args ->> 'account_id' ASC,
                    transactions.timestamp DESC
                ) as txs ON projects.id = txs.account_id
                "#,
                "ORDER BY txs.timestamp DESC",
            ),
            Sort::NameAsc => ("", "ORDER BY NULLIF(projects.name, '') ASC NULLS LAST"),
            Sort::NameDesc => ("", "ORDER BY NULLIF(projects.name, '') DESC NULLS LAST"),
            Sort::RecentAsc => (
                r#"
                LEFT JOIN (
                  SELECT
                    DISTINCT ON (
                      COALESCE(
                        transactions.args ->> 'account_id',
                        transactions.args ->> 'project_id'
                      )
                    ) COALESCE(
                      transactions.args ->> 'account_id',
                      transactions.args ->> 'project_id'
                    ) as account_id,
                    transactions.method_name,
                    transactions.timestamp
                  FROM
                    transactions
                  WHERE
                    COALESCE(
                      transactions.args ->> 'account_id',
                      transactions.args ->> 'project_id'
                    ) IS NOT NULL
                    AND transactions.method_name IN ('add_project', 'edit_project')
                  ORDER BY
                    COALESCE(
                      transactions.args ->> 'account_id',
                      transactions.args ->> 'project_id'
                    ) ASC,
                    transactions.timestamp ASC
                ) as txs ON projects.id = txs.account_id
                "#,
                "ORDER BY txs.timestamp ASC",
            ),
            Sort::RecentDesc => (
                r#"
                LEFT JOIN (
                  SELECT
                    DISTINCT ON (
                      COALESCE(
                        transactions.args ->> 'account_id',
                        transactions.args ->> 'project_id'
                      )
                    ) COALESCE(
                      transactions.args ->> 'account_id',
                      transactions.args ->> 'project_id'
                    ) as account_id,
                    transactions.method_name,
                    transactions.timestamp
                  FROM
                    transactions
                  WHERE
                    COALESCE(
                      transactions.args ->> 'account_id',
                      transactions.args ->> 'project_id'
                    ) IS NOT NULL
                    AND transactions.method_name IN ('add_project', 'edit_project')
                  ORDER BY
                    COALESCE(
                      transactions.args ->> 'account_id',
                      transactions.args ->> 'project_id'
                    ) ASC,
                    transactions.timestamp DESC
                ) as txs ON projects.id = txs.account_id
                "#,
                "ORDER BY txs.timestamp DESC",
            ),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Params {
    #[serde(default)]
    pub sort: Sort,
    #[serde(default, deserialize_with = "set_deserialize")]
    pub vertical: Option<HashSet<String>>,
    #[serde(default, deserialize_with = "set_deserialize")]
    pub integration: Option<HashSet<String>>,
    #[serde(default, deserialize_with = "set_deserialize")]
    pub dev: Option<HashSet<String>>,
    #[serde(default, deserialize_with = "set_deserialize")]
    pub stage: Option<HashSet<String>>,
    #[serde(default, deserialize_with = "size_deserialize")]
    pub size: Option<HashSet<(u32, u32)>>,
    #[serde(default, deserialize_with = "set_deserialize")]
    pub distribution: Option<HashSet<String>>,
    pub from: Option<u32>,
    pub limit: Option<u32>,
    #[serde(rename = "q")]
    pub search: Option<String>,
}

#[debug_handler(state = AppState)]
pub async fn all_projects(
    Query(params): Query<Params>,
    State(AppState { pool, .. }): State<AppState>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let mut builder = sqlx::QueryBuilder::new(
        r#"
        SELECT
          projects.id
        FROM
          projects
        "#,
    );

    let (join, order_by) = params.sort.get_order_by();

    builder.push(join);

    let mut has_where = false;

    if let Some(verticals) = params.vertical {
        let verticals = verticals.into_iter().collect::<Vec<_>>();
        builder.push("WHERE projects.vertical ?| ");
        builder.push_bind(verticals);
        has_where = true;
    }

    if let Some(integration) = params.integration {
        if has_where {
            builder.push(" AND ");
        } else {
            builder.push("WHERE ");
            has_where = true;
        }
        builder.push("projects.integration = ANY (");
        builder.push_bind(integration.into_iter().collect::<Vec<_>>());
        builder.push(") ");
    }

    if let Some(stage) = params.stage {
        if has_where {
            builder.push(" AND ");
        } else {
            builder.push("WHERE ");
            has_where = true;
        }
        builder.push("projects.stage = ANY (");
        builder.push_bind(stage.into_iter().collect::<Vec<_>>());
        builder.push(") ");
    }

    if let Some(dev) = params.dev {
        if has_where {
            builder.push(" AND ");
        } else {
            builder.push("WHERE ");
            has_where = true;
        }
        builder.push("projects.dev = ANY (");
        builder.push_bind(dev.into_iter().collect::<Vec<_>>());
        builder.push(") ");
    }

    if let Some(sizes) = params.size {
        if has_where {
            builder.push(" AND ");
        } else {
            builder.push("WHERE ");
            has_where = true;
        }
        builder.push(" ( ");
        let statement = r#"
        (
          SELECT
            COUNT(*)
          FROM
            jsonb_object_keys(projects.team)
        ) + array_length(projects.founders, 1) BETWEEN
        "#;

        for (i, (from, to)) in sizes.into_iter().enumerate() {
            if i > 0 {
                builder.push(" OR ");
            }
            builder.push(statement);
            builder.push_bind(from as i32);
            builder.push(" AND ");
            builder.push_bind(to as i32);
        }
        builder.push(" ) ");
    }

    if let Some(distribution) = params.distribution {
        if has_where {
            builder.push(" AND ");
        } else {
            builder.push("WHERE ");
            has_where = true;
        }
        builder.push("projects.distribution = ANY (");
        builder.push_bind(distribution.into_iter().collect::<Vec<_>>());
        builder.push(") ");
    }

    if let Some(search) = params.search {
        if has_where {
            builder.push(" AND ");
        } else {
            builder.push(" WHERE ");
        }
        let search = format!("%{search}%");
        builder.push(" (projects.name ILIKE ");
        builder.push_bind(search.clone());
        builder.push(" OR projects.id ILIKE ");
        builder.push_bind(search.clone());
        builder.push(" OR projects.tagline ILIKE ");
        builder.push_bind(search.clone());
        builder.push(" OR projects.description ILIKE ");
        builder.push_bind(search);
        builder.push(") ");
    }

    builder.push(format!(" {order_by}"));

    if let Some(limit) = params.limit {
        builder.push(" LIMIT ");
        builder.push_bind(limit as i32);
    }

    if let Some(from) = params.from {
        builder.push(" OFFSET ");
        builder.push_bind(from as i32);
    }

    let result = builder.build().fetch_all(&pool).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get projects: {e}"),
        )
    })?;

    Ok(Json({
        let mut seen = HashSet::<String>::new();
        result
            .into_iter()
            .filter_map(|r| {
                let id: String = r.get("id");
                (!seen.contains(&id)).then_some({
                    seen.insert(id.clone());
                    id
                })
            })
            .collect()
    }))
}

#[debug_handler(state = AppState)]
pub async fn get_similar_projects(
    Path(account_id): Path<String>,
    State(AppState { pool, .. }): State<AppState>,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let project = sqlx::query!(
        r#"
        SELECT
          projects.id
        FROM
          (
            SELECT
              *
            FROM
              projects
            WHERE
              id = $1
          ) AS target,
          (
            SELECT
              *
            FROM
              projects
            WHERE
              id != $1
          ) AS projects
        WHERE
          (
            SELECT
              COUNT(*)
            FROM
              jsonb_object_keys(projects.vertical) AS v
            WHERE
              target.vertical ? v
          ) > 0
          OR (
            SELECT
              COUNT(*)
            FROM
              unnest(projects.product_type) AS v
            WHERE
              v = ANY (target.product_type)
          ) > 0
          OR projects.stage ILIKE target.stage
          OR projects.dev ILIKE target.dev
          OR projects.distribution ILIKE target.distribution
        ORDER BY
          (
            SELECT
              COUNT(*)
            FROM
              unnest(projects.product_type) AS v
            WHERE
              v = ANY (target.product_type)
          ) + (
            SELECT
              COUNT(*)
            FROM
              jsonb_object_keys(projects.vertical) AS v
            WHERE
              target.vertical ? v
          ) + (
            projects.stage ILIKE target.stage
          ) :: int + (
            projects.dev ILIKE target.dev
          ) :: int + (
            projects.distribution ILIKE target.distribution
          ) :: int DESC
        "#,
        account_id
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get project: {e}"),
        )
    })?;

    Ok(Json(project.into_iter().map(|p| p.id).collect()))
}

#[debug_handler(state = AppState)]
async fn get_completion(
    State(AppState { pool, .. }): State<AppState>,
) -> Result<Json<Completion>, (StatusCode, String)> {
    let list = sqlx::query_as!(
        CompletionPair,
        r#"
        SELECT
          projects.id,
          projects.completion
        FROM
          projects
        ORDER BY
          projects.completion DESC
        "#
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get project completions: {e}"),
        )
    })?;

    let avg = list.iter().map(|c| c.completion).sum::<f64>() / list.len() as f64;

    Ok(Json(Completion { avg, list }))
}

pub fn create_router() -> Router<AppState> {
    Router::new()
        .route("/", get(all_projects))
        .route("/completion", get(get_completion))
        .route("/:account_id/similar", get(get_similar_projects))
}
