use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use futures::stream::BoxStream;
use futures::{Stream, StreamExt};

use crate::conversation::message::Message;
use crate::conversation::token_usage::ProviderUsage;

pub(crate) struct FirstContentWindow {
    pub first_content_line_read_at: DateTime<Utc>,
    pub next_line_read_at: DateTime<Utc>,
    pub reported: Option<DateTime<Utc>>,
}

pub(crate) async fn first_content_window<P, R>(
    events: &str,
    first_content_line: usize,
    parse: P,
) -> FirstContentWindow
where
    P: FnOnce(BoxStream<'static, anyhow::Result<String>>) -> R,
    R: Stream<Item = anyhow::Result<(Option<Message>, Option<ProviderUsage>)>>,
{
    let read_at = Arc::new(Mutex::new(Vec::new()));
    let recorder = read_at.clone();
    let lines: Vec<String> = events.lines().map(str::to_string).collect();
    let stream = futures::stream::iter(lines)
        .map(move |line| {
            recorder.lock().unwrap().push(Utc::now());
            Ok(line)
        })
        .boxed();

    let results: Vec<_> = Box::pin(parse(stream)).collect().await;
    let reported = results
        .into_iter()
        .filter_map(|result| result.expect("stream must parse").1)
        .find_map(|usage| usage.stats.and_then(|stats| stats.first_content_at));

    let read_at = read_at.lock().unwrap();
    FirstContentWindow {
        first_content_line_read_at: read_at[first_content_line],
        next_line_read_at: read_at[first_content_line + 1],
        reported,
    }
}

impl FirstContentWindow {
    pub fn assert_reported_at_first_content(&self) {
        let reported = self.reported.expect("first content time must be reported");
        assert!(
            self.first_content_line_read_at <= reported && reported <= self.next_line_read_at,
            "first content time must be taken when the first content line is read"
        );
    }
}
