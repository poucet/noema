use crate::{VoiceAgent, VoiceEvent};

pub struct VoiceCoordinator {
    agent: VoiceAgent,
    pending_messages: Vec<String>,
    is_listening: bool,
    is_transcribing: bool,
}

impl VoiceCoordinator {
    pub fn new(agent: VoiceAgent) -> Self {
        Self {
            agent,
            pending_messages: Vec::new(),
            is_listening: false,
            is_transcribing: false,
        }
    }

    pub fn is_listening(&self) -> bool {
        self.is_listening
    }

    pub fn is_transcribing(&self) -> bool {
        self.is_transcribing
    }

    /// Poll for voice events and return messages to send.
    /// If `buffering` is true, transcriptions are queued instead of returned.
    /// Returns (messages_to_send, errors)
    pub fn process(&mut self, buffering: bool) -> (Vec<String>, Vec<String>) {
        let mut messages = Vec::new();
        let mut errors = Vec::new();

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
                        if buffering {
                            self.pending_messages.push(text);
                        } else {
                            messages.push(text);
                        }
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

        if !buffering && !self.pending_messages.is_empty() {
            messages.append(&mut self.pending_messages);
        }

        (messages, errors)
    }
}
