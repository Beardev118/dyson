use aide::openapi::{self, OpenApi};
use anyhow::Result;
use axum::{Extension, Server};
use redis::aio::ConnectionManager;
use std::{env, net::SocketAddr};

use crate::{routes, shutdown::Shutdown};

#[allow(clippy::redundant_pub_crate)]
pub(crate) async fn start(redis_pool: ConnectionManager) -> Result<()> {
	let mut openapi = OpenApi {
		info: openapi::Info {
			title: "Dyson".to_string(),
			version: option_env!("GIT_REV")
				.unwrap_or_else(|| env!("STATIC_BUILD_DATE"))
				.to_string(),
			..openapi::Info::default()
		},
		..OpenApi::default()
	};

	let shutdown = Shutdown::new()?;
	let router = routes::handler().finish_api(&mut openapi);

	let router = router
		.layer(Extension(openapi))
		.layer(shutdown.extension())
		.layer(Extension(redis_pool));

	let addr = SocketAddr::from((
		[0, 0, 0, 0],
		env::var("PORT").map_or(Ok(8000), |p| p.parse())?,
	));

	tracing::info!("Starting server on {addr}...");
	Server::bind(&addr)
		.serve(router.into_make_service())
		.with_graceful_shutdown(shutdown.handle())
		.await?;

	Ok(())
}
