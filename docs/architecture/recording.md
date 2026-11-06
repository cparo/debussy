# Recording

## Input monitoring

Each armed track has a monitor toggle, off by default. When on, the track's live input passes
through its part of the processing graph to the outputs while recording ([ADR-0021](../adrs/0021-inputs-can-be-monitored-through-the-software-per.md)).

Monitored input is not delayed for delay compensation; its latency is the device round trip alone ([ADR-0032](../adrs/0032-monitored-input-is-not-delayed-for-delay.md)).

## Recording alignment

Recorded audio is placed on the timeline shifted by the input and output devices' reported
latencies, so that a take lines up with the playback the performer heard ([ADR-0022](../adrs/0022-the-engine-compensates-recording-and-effect.md)).
