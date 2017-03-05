#! /bin/sh
cargo fuzz run server_basic -- -dict=fuzzer_dict -only_ascii=1 -timeout=60 $@
