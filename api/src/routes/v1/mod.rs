use aide::axum::{
	routing::{get, post},
	ApiRouter,
};
use axum::Extension;
use axum_jsonschema::Json;
use ensemble::{types::ToJson, Model};
use redis::{aio::ConnectionManager, AsyncCommands};
use schemars::{JsonSchema, _serde_json::Value};
use serde_json::json;
use url::Url;

use crate::{
	axum::extractors::{prediction::AuthenticatedPrediction, AuthenticatedUser},
	config,
	errors::RouteError,
	models::{Prediction, PredictionStatus, WebhookEvent},
	webhooks::WebhookSender,
};

pub fn handler() -> ApiRouter {
	ApiRouter::new().nest(
		"/v1",
		ApiRouter::new()
			.api_route("/predictions", get(get_predictions))
			.api_route("/predictions/:id", get(get_prediction))
			.api_route("/predictions", post(create_prediction))
			.api_route("/predictions/:id/cancel", post(cancel_prediction)),
	)
}

async fn get_predictions(
	AuthenticatedUser(mut user): AuthenticatedUser,
) -> Result<Json<Vec<Prediction>>, RouteError> {
	let predictions = user.predictions().await.unwrap();

	Ok(Json(predictions.clone()))
}

#[allow(clippy::unused_async)]
async fn get_prediction(
	AuthenticatedPrediction(prediction): AuthenticatedPrediction,
) -> Json<Prediction> {
	Json(prediction)
}

#[derive(Debug, serde::Deserialize, JsonSchema)]
struct CreatePredictionRequest {
	/// The ID of the model version that you want to run.
	version: String,
	/// The model's input as a JSON object. The input depends on what model you are running.
	input: Value,
	/// An HTTPS URL for receiving a webhook when the prediction has new output. The webhook will be a POST request where the request body is the same as the response body of the [get prediction](#predictions.get) operation.
	webhook: Option<Url>,
	/// By default, we will send requests to your webhook URL whenever there are new logs, new outputs, or the prediction has finished. You can change which events trigger webhook requests by specifying `webhook_events_filter` in the prediction request.
	///
	/// * `start`: immediately on prediction start
	/// * `output`: each time a prediction generates an output (note that predictions can generate multiple outputs)
	/// * `logs`: each time log output is generated by a prediction
	/// * `completed`: when the prediction reaches a terminal state (succeeded/canceled/failed)
	///
	/// Requests for event types `output` and `logs` will be sent at most once every 500ms. If you request `start` and `completed` webhooks, then they'll always be sent regardless of throttling.
	webhook_events_filter: Option<Vec<WebhookEvent>>,
}

async fn create_prediction(
	AuthenticatedUser(mut user): AuthenticatedUser,
	Extension(mut redis): Extension<ConnectionManager>,
	Json(req): Json<CreatePredictionRequest>,
) -> Result<Json<Prediction>, RouteError> {
	tracing::trace!(req = ?req, "User @{} requested a prediction", user.username);

	let prediction = user
		.predictions
		.create(Prediction {
			version: req.version,
			input: req.input.into(),
			webhook_url: req.webhook.map(Url::into),
			webhook_filter: req
				.webhook_events_filter
				.unwrap_or_default()
				.into_iter()
				.map(Into::into)
				.collect::<Vec<_>>()
				.to_json(),
			..Default::default()
		})
		.await
		.unwrap();

	if let Err(e) = redis
		.lpush::<_, _, ()>(config::REDIS_PREDICTION_QUEUE, prediction.id.to_string())
		.await
	{
		tracing::error!(error = ?e, "Failed to push prediction to queue");

		prediction.delete().await?;
		return Err(RouteError::internal_error().set_error(e.into()));
	}

	Ok(Json(prediction))
}

async fn cancel_prediction(
	AuthenticatedPrediction(mut prediction): AuthenticatedPrediction,
	Extension(mut redis): Extension<ConnectionManager>,
) -> Result<Json<Prediction>, RouteError> {
	tracing::trace!(
		"User {} requested to cancel prediction {}",
		prediction.user.value,
		prediction.id
	);

	if !matches!(
		prediction.status,
		PredictionStatus::Starting | PredictionStatus::Processing,
	) {
		return Err(RouteError::bad_request()
			.set_data(json!({ "prediction": prediction }))
			.set_message("Prediction is not cancellable"));
	}

	if let Err(e) = redis
		.lpush::<_, _, ()>(config::REDIS_CANCEL_QUEUE, prediction.id.to_string())
		.await
	{
		tracing::error!(error = ?e, "Failed to push prediction to cancel queue");

		return Err(RouteError::internal_error().set_error(e.into()));
	}

	prediction.status = PredictionStatus::Cancelled;
	prediction.save().await?;

	let wh_prediction = prediction.clone();
	tokio::spawn(async move {
		let sender = WebhookSender::new().unwrap();
		sender.finished(&wh_prediction).await.unwrap();
	});

	Ok(Json(prediction))
}
