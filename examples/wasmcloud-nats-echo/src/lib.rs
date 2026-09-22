mod bindings {
    use crate::Component;

    wit_bindgen::generate!({
        world: "hello",
        path: "wit",
        generate_all,
    });

    export!(Component);
}

use bindings::wasmcloud::nats::{
    core,
    types::{HeaderEntry, NatsMessage},
};
use otel_wasi::ResultWithSlug;

const PUBLISH_FAILED_SLUG: &str = "nats-publish-failed";

struct Component;

impl bindings::exports::wasmcloud::nats::core_handler::Guest for Component {
    #[otel_wasi::wasi_instrument(
        service = "otel-wasi-nats-echo-example",
        name = "handle-message",
        export,
        attributes(
            "messaging.system" = "nats",
            "component.kind" = "wasmcloud-nats-handler"
        )
    )]
    async fn handle_message(msg: NatsMessage) -> Result<(), otel_wasi::Error> {
        // Dynamic attributes for the current root span.
        otel_wasi::attribute!(
            "messaging.destination.name" = msg.subject.clone(),
            "messaging.message.body.size" = msg.body.len() as i64,
            "messaging.reply_to.present" = msg.reply_to.is_some(),
        );

        if let Some(reply_to) = &msg.reply_to {
            otel_wasi::attribute!("messaging.reply_to.destination.name" = reply_to.clone(),);
        }

        publish_reply(&msg).await
    }
}

#[tracing::instrument(name = "publish-reply", level = "info", skip(msg))]
async fn publish_reply(msg: &NatsMessage) -> Result<(), otel_wasi::Error> {
    // Because this function is instrumented with normal `tracing`, this writes
    // local debugging detail to the child `publish-reply` span.
    otel_wasi::attribute!("messaging.message.body.size" = msg.body.len() as i64,);

    // Roll up incident query context to the entrypoint span without passing the
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

    let headers = otel_wasi::current_propagation_context().map(|context| {
        let mut headers = vec![HeaderEntry {
            name: "traceparent".into(),
            value: context.traceparent,
        }];
        if let Some(value) = context.tracestate {
            headers.push(HeaderEntry {
                name: "tracestate".into(),
                value,
            });
        }
        headers
    });
    let result = core::publish(NatsMessage {
        subject: reply_to,
        body: msg.body.clone(),
        reply_to: None,
        headers,
    })
    .await
    .map_err(|error| format!("{error:?}"));

    // Attach a slug to the error. The #[wasi_instrument(export)] macro
    // on handle_message will record the slug+message on the span and
    // convert back to String for the WIT contract.
    result.error_with_slug(PUBLISH_FAILED_SLUG)
}
