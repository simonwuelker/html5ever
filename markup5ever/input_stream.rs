use tendril::StrTendril;

use crate::buffer_queue::BufferQueue;

/// <https://html.spec.whatwg.org/#input-stream>
#[derive(Default)]
pub struct InputStream {
    input: BufferQueue,
    // TODO: Add a decoder here
}

impl InputStream {
    pub fn append(&self, data: StrTendril) {
        self.input.push_back(data);
    }

    pub fn code_points(&self) -> &BufferQueue {
        &self.input
    }

    /// Remove all input from the stream
    pub fn clear(&self) {
        self.input.clear();
    }
}

pub struct DecodingParser<Sink> {
    /// Data received from `document.write`
    script_input: BufferQueue,
    input_stream: InputStream,
    /// Something that can consume buffered input.
    /// In practice this is likely either html- or an xml tokenizer.
    input_sink: Sink,
}

impl<Sink> DecodingParser<Sink>
where
    Sink: InputSink,
{
    pub fn new(sink: Sink) -> Self {
        Self {
            script_input: Default::default(),
            input_stream: InputStream::default(),
            input_sink: sink,
        }
    }

    pub fn sink(&self) -> &Sink {
        &self.input_sink
    }

    pub fn input_stream(&self) -> &InputStream {
        &self.input_stream
    }

    /// Return an iterator that can be used to drive the parser
    pub fn parse(&self) -> impl Iterator<Item = ParserAction<Sink::Handle>> + '_ {
        self.input_sink.feed(self.input_stream.code_points())
    }

    /// Returns an iterator that can be used to drive the parser
    pub fn document_write<'a>(
        &'a self,
        input: &'a BufferQueue,
    ) -> impl Iterator<Item = ParserAction<Sink::Handle>> + use<'a, Sink> {
        debug_assert!(
            self.script_input.is_empty(),
            "Should not parse input from document.write while the parser is suspended"
        );

        self.input_sink.feed(&input)
    }

    /// End a `document.write` transaction, appending any input that was not yet parsed to the
    /// current insertion point, behind any input that was received reentrantly during this transaction.
    pub fn push_script_input(&self, input: &BufferQueue) {
        while let Some(chunk) = input.pop_front() {
            self.script_input.push_back(chunk);
        }
    }

    /// Notifies the parser that it has been unblocked and parsing can resume
    pub fn notify_parser_blocking_script_loaded(&self) {
        // Move pending script input to the front of the input stream
        self.script_input.swap_with(&self.input_stream.input);
        while let Some(chunk) = self.script_input.pop_front() {
            self.input_stream.input.push_back(chunk);
        }
    }
}

pub enum ParserAction<Handle> {
    HandleScript(Handle),
}

pub trait InputSink {
    type Handle;

    fn feed<'a>(
        &'a self,
        input: &'a BufferQueue,
    ) -> impl Iterator<Item = ParserAction<Self::Handle>> + 'a;
}

impl<T> ParserAction<T> {
    pub fn map_script<U, F>(self, f: F) -> ParserAction<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            Self::HandleScript(script) => ParserAction::HandleScript(f(script)),
        }
    }
}
