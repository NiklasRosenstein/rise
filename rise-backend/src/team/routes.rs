use axum::{
    routing::{get, post, put, delete},
    Router,
};
use crate::state::AppState;
use super::handlers;

pub fn team_routes() -> Router<AppState> {
    Router::new()
        .route("/teams", post(handlers::create_team))
        .route("/teams", get(handlers::list_teams))
        .route("/teams/:id", get(handlers::get_team))
        .route("/teams/:id", put(handlers::update_team))
        .route("/teams/:id", delete(handlers::delete_team))
}
