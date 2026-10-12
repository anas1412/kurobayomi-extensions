# Comix is parked

The site rotates the keys its API requests are signed with whenever it rebuilds (last seen: build `tmonyg` retired, answer `key_retired`). Until the keys are re-captured (`capture.js`, see README) the source cannot list anything, so it is not offered in the index. Move this folder back to `sources/comix`, replace `src/keys/tmonyg.json`, run `tools/build.sh` and `tools/make-index.py` to bring it back.
