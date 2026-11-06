# WAV fixtures

Generated on macOS with `say` and `afconvert`, to test reading files from another tool:

```sh
say -o src.aiff --data-format=BEI16@8000 "Debussy"
for f in LEI16 LEI24 LEI32 LEF32 LEF64; do afconvert -f WAVE -d $f src.aiff mono_$f.wav; done
afconvert -f WAVE -d LEI24 -c 2 src.aiff stereo_LEI24.wav
afconvert -f WAVE -d LEF32 -c 2 src.aiff stereo_LEF32.wav
afconvert -f RF64 -d LEF32 src.aiff mono_LEF32_junk.wav   # small, so RIFF with a JUNK reserve
```

The source is 16-bit, so every fixture decodes to the same samples.
