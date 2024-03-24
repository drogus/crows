#!/bin/bash

set -e

declare -a CRATES=(service shared utils macros bindings wasm worker coordinator cli)

for crate in "${CRATES[@]}" ; do
	pushd "${crate}"
	if [ $DRY_RUN == 1 ] ; then
		cargo publish --dry-run
	else
		cargo publish
	fi
  popd
done

