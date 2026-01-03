#!/bin/bash

crate=$1
pattern=$2

cargo check -p "${crate}" 2>&1 \
| awk -v pattern="${pattern}" '
    /^warning/ || /^error/ {
        if (inmsg && file ~ pattern) {
            print message"\n===============\n"
        }
        inmsg = 1
        start = 1
        message = $0
        next
    }
    start {
        file = $NF
        start = 0
        message = message"\n"$0
        next
    }
    inmsg {
        message = message"\n"$0
    }
    END {
        if (inmsg && file ~ pattern) {
            print message"\n===============\n"
        }
    }'
