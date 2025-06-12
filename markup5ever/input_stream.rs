use std::iter;
use std::cell::RefCell;

use tendril::StrTendril;

use crate::buffer_queue::{self, BufferQueue};

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
    /// Data from a `document.write` call that is *currently* executing.
    ///
    /// As soon as the call is finished, any remaining input goes into the `script_input` field.
    document_write_input: RefCell<Vec<BufferQueue>>,
    /// Pending input from `document.write` calls that blocked the parser before they could finish
    /// parsing all their input.
    ///
    /// As soon as the `<script>` that made these calles finished executing, this data goes to the
    /// front of the input stream.
    script_input: BufferQueue,
    input_stream: InputStream,
    /// Something that can consume buffered input.
    /// In practice this is likely either a html- or an xml tokenizer.
    input_sink: Sink,
}

impl<Sink> DecodingParser<Sink>
where
    Sink: InputSink,
{
    pub fn new(sink: Sink) -> Self {
        Self {
            document_write_input: Default::default(),
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
        // Either there is a pending parser blocking script (and this parser should therefore be suspended)
        // or you forgot to call notify_parser_blocking_script_loaded!
        debug_assert!(self.script_input.is_empty());

        self.input_sink.feed(self.input_stream.code_points())
    }

    /// Returns an iterator that can be used to drive the parser
    pub fn document_write<'a>(
        &'a self,
        input: StrTendril,
    ) -> impl Iterator<Item = ParserAction<Sink::Handle>> + use<'a, Sink> {
        debug_assert!(
            self.script_input.is_empty(),
            "Should not parse input from document.write while the parser is suspended"
        );
        let buffer_queue = BufferQueue::default();
        buffer_queue.push_back(input);
        self.document_write_input.borrow_mut().push(buffer_queue);

        iter::from_fn(|| {
            let document_write_stack =  self.document_write_input.borrow();
            let active_document_write_state = document_write_stack.last().expect("no transaction?");

            // Curiously, this temporary *is* required.
            let x = self.input_sink.feed(&active_document_write_state).next();
            x
        })
    }

    // pub fn perform_document_writing(&self) -> Option<ParserAction<Sink::Handle>> {
    //     self.script_input.push_back(input);
    // }

    /// End a `document.write` transaction, appending any input that was not yet parsed to the
    /// current insertion point, behind any input that was received reentrantly during this transaction.
    ///
    /// A `document.write` call may be unable to immediately parse all input if a pending parser blocking
    /// script was inserted.
    pub fn end_document_write_transaction(&self) {
        let input = self.document_write_input.borrow_mut().pop().expect("no active transaction?");

        while let Some(chunk) = input.pop_front() {
            self.script_input.push_back(chunk);
        }
    }

    pub fn push_script_input(&self, input: StrTendril) {
        self.script_input.push_back(input);
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
