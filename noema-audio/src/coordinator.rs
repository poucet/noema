use crate::{VoiceAgent, VoiceEvent};

pub struct VoiceCoordinator {
    agent: VoiceAgent,
    pending_messages: Vec<String>,
    is_listening: bool,
    is_transcribing: bool,
    is_buffering: bool,
}

impl VoiceCoordinator {
    pub fn new(agent: VoiceAgent) -> Self {
        Self {
            agent,
            pending_messages: Vec::new(),
            is_listening: false,
            is_transcribing: false,
            is_buffering: false,
        }
    }

    pub fn is_listening(&self) -> bool {
        self.is_listening
    }

    pub fn is_transcribing(&self) -> bool {
        self.is_transcribing
    }

    pub fn is_buffering(&self) -> bool {
        self.is_buffering
    }

    pub fn buffered_count(&self) -> usize {
        self.pending_messages.len()
    }

    /// Poll for voice events and return messages to send.
    /// If `buffering` is true, transcriptions are queued instead of returned.
    /// Returns (message_to_send, errors) - buffered messages are concatenated into one
    pub fn process(&mut self, buffering: bool) -> (Option<String>, Vec<String>) {
        let mut errors = Vec::new();

        self.is_buffering = buffering;

        while let Some(event) = self.agent.try_recv() {
            match event {
                VoiceEvent::ListeningStarted => {
                    self.is_listening = true;
                    self.is_transcribing = false;
                }
                VoiceEvent::Transcribing => {
                    self.is_listening = false;
                    self.is_transcribing = true;
                }
                VoiceEvent::Transcription(text) => {
                    self.is_listening = false;
                    self.is_transcribing = false;
                    if !text.trim().is_empty() {
                        self.pending_messages.push(text);
                    }
                }
                VoiceEvent::Error(e) => {
                    self.is_listening = false;
                    self.is_transcribing = false;
                    errors.push(e);
                }
                _ => {}
            }
        }

        // Only flush pending messages when not buffering
        let message = if !buffering && !self.pending_messages.is_empty() {
            let combined = self.pending_messages.join(" ");
            self.pending_messages.clear();
            Some(combined)
        } else {
            None
        };

        (message, errors)
    }
}
