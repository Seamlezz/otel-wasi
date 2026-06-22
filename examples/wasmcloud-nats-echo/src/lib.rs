mod bindings {
    use crate::Component;

    wit_bindgen::generate!({
        world: "hello",
        path: "wit",
        generate_all,
    });

    export!(Component);
}

use bindings::wasmcloud::messaging::{consumer, types::BrokerMessage};
use opentelemetry::trace::Status;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

const PUBLISH_FAILED_SLUG: &str = "nats-publish-failed";

struct Component;

impl bindings::exports::wasmcloud::messaging::handler::Guest for Component {
    #[otel_wasi::wasi_instrument(
        service = "otel-wasi-nats-echo-example",
        name = "handle-message",
        error_slug = "nats-handler-failed",
        attributes(
            "messaging.system" = "nats",
            "component.kind" = "wasmcloud-messaging-handler"
        )
    )]
    fn handle_message(msg: BrokerMessage) -> Result<(), String> {
        // Dynamic attributes for the current root span.
        otel_wasi::attribute!(
            "messaging.destination.name" = msg.subject.clone(),
            "messaging.message.body.size" = msg.body.len() as i64,
            "messaging.reply_to.present" = msg.reply_to.is_some(),
        );

        if let Some(reply_to) = &msg.reply_to {
            otel_wasi::attribute!("messaging.reply_to.destination.name" = reply_to.clone(),);
        }

        publish_reply(&msg)
    }
}

#[tracing::instrument(name = "publish-reply", level = "info", skip(msg))]
fn publish_reply(msg: &BrokerMessage) -> Result<(), String> {
    // Because this function is instrumented with normal `tracing`, this writes
    // to the child `publish-reply` span rather than the root `handle-message` span.
    otel_wasi::attribute!(
        "messaging.system" = "nats",
        "messaging.message.body.size" = msg.body.len() as i64,
    );

    let Some(reply_to) = msg.reply_to.clone() else {
        Span::current().set_status(Status::Ok);
        return Ok(());
    };

    otel_wasi::attribute!("messaging.destination.name" = reply_to.clone());

    let result = consumer::publish(&BrokerMessage {
        subject: reply_to,
        body: msg.body.clone(),
        reply_to: None,
    });

    match &result {
        Ok(()) => Span::current().set_status(Status::Ok),
        Err(e) => record_current_error(PUBLISH_FAILED_SLUG, e),
    }

    result
}

fn record_current_error(slug: &'static str, message: &str) {
    otel_wasi::attribute!(
        "error" = true,
        "exception.slug" = slug,
        "exception.message" = message.to_string(),
    );
    Span::current().set_status(Status::error(message.to_string()));
}
