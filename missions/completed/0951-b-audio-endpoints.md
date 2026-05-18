# Mission: /v1/audio Endpoints

## Status

Open

## RFC

RFC-0951 (Economics): Extended API Endpoints

## Dependencies

None

## Acceptance Criteria

- [ ] POST /v1/audio/transcriptions accepts multipart/form-data
- [ ] POST /v1/audio/speech accepts TextToSpeechRequest
- [ ] Supports OpenAI Whisper model
- [ ] Supports OpenAI TTS model
- [ ] /v1/audio/transcriptions returns transcribed text
- [ ] /v1/audio/speech returns audio bytes
- [ ] Streaming works for TTS
- [ ] Audio format validation (MIME type, size limits)
- [ ] Error handling follows RFC-0920 taxonomy
- [ ] Works in litellm-mode (reqwest)
- [ ] Works in any-llm-mode (py_bridge)
- [ ] Unit tests pass
- [ ] Integration tests pass

## Claimant

Unclaimed

## Pull Request

None

## Notes

- Whisper: https://api.openai.com/v1/audio/transcriptions
- TTS: https://api.openai.com/v1/audio/speech
- Multipart form upload for transcription
- Streaming audio response for TTS
