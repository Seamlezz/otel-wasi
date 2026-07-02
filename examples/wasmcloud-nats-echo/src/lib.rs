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
use otel_wasi::ResultWithSlug;

const PUBLISH_FAILED_SLUG: &str = "nats-publish-failed";

struct Component;

impl bindings::exports::wasmcloud::messaging::handler::Guest for Component {
    #[otel_wasi::wasi_instrument(
        service = "otel-wasi-nats-echo-example",
        name = "handle-message",
        export,
        attributes(
            "messaging.system" = "nats",
            "component.kind" = "wasmcloud-messaging-handler"
        )
    )]
    fn handle_message(msg: BrokerMessage) -> Result<(), otel_wasi::Error> {
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
fn publish_reply(msg: &BrokerMessage) -> Result<(), otel_wasi::Error> {
    // Because this function is instrumented with normal `tracing`, this writes
    // local debugging detail to the child `publish-reply` span.
    otel_wasi::attribute!("messaging.message.body.size" = msg.body.len() as i64,);

    // Roll up incident-query context to the entrypoint span without passing the
    // span through every helper.
    otel_wasi::main_attribute!(
        "messaging.reply.publish_attempted" = true,
        "messaging.message.body.size" = msg.body.len() as i64,
    );

    let Some(reply_to) = msg.reply_to.clone() else {
        otel_wasi::main_attribute!("messaging.reply.skipped" = true);
        return Ok(());
    };

    otel_wasi::attribute!("messaging.destination.name" = reply_to.clone());
    otel_wasi::main_attribute!("messaging.reply.destination.name" = reply_to.clone());

    let result = consumer::publish(&BrokerMessage {
        subject: reply_to,
        body: msg.body.clone(),
        reply_to: None,
    });

    // Attach a slug to the error — the #[wasi_instrument(export)] macro
    // on handle_message will record the slug+message on the span and
    // convert back to String for the WIT contract.
    result.error_with_slug(PUBLISH_FAILED_SLUG)
}
