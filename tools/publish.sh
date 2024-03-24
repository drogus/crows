#!/bin/bash

set -e

declare -a CRATES=(service utils wasm macros bindings shared worker coordinator cli)

for crate in "${CRATES[@]}" ; do
	cd "${crate}"
	if [ $DRY_RUN == 1 ] ; then
		cargo publish --dry-run
	else
		cargo publish
	fi
done

