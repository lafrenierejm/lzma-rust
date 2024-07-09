#!/bin/bash

IFS=$'\n'

find_replace() {
	line="$1"
	replace="$2"
	arg="$3"
	path="$4"
	given_arg="$(echo "$line" | cut -d '<' -f2- | sed 's/>$//g')"
	sed -i "s/${line}/${replace}(${arg}, ${given_arg})/g" "$path"
}

for rs in $(find ./src -type f -name "*.rs"); do

	for line in $(grep -o 'WriteResult<.*>' "$rs"); do
		find_replace "$line" 'crate::io::write_result\!' "W" "$rs"
	done

	for line in $(grep -o 'WriteRangeEncoderBufferResult<.*>' "$rs"); do
		find_replace "$line" 'crate::io::write_result\!' "RangeEncoderBuffer" "$rs"
	done

	for line in $(grep -o 'WriteCountingWriterResult<.*>' "$rs"); do
		find_replace "$line" 'crate::io::write_result\!' "CountingWriter<W>" "$rs"
	done

	for line in $(grep -o 'WriteSelfResult<.*>' "$rs"); do
		find_replace "$line" 'crate::io::write_result\!' "Self" "$rs"
	done

	for line in $(grep -o 'ReadExactResult<.*>' "$rs"); do
		find_replace "$line" 'crate::io::read_exact_result\!' "R" "$rs"
	done

	for line in $(grep -o 'ReadSelfExactResult<.*>' "$rs"); do
		find_replace "$line" 'crate::io::read_exact_result\!' "Self" "$rs"
	done

	if [[ "$(grep -o 'WriteResult' "$rs")" != "" ]]; then
		sed -i 's/WriteResult,//g' "$rs"
		sed -i 's/, WriteResult//g' "$rs"
		sed -i 's/WriteResult//g' "$rs"
	fi

	if [[ "$(grep -o 'WriteRangeEncoderBufferResult' "$rs")" != "" ]]; then
		sed -i 's/WriteRangeEncoderBufferResult,//g' "$rs"
		sed -i 's/, WriteRangeEncoderBufferResult//g' "$rs"
		sed -i 's/WriteRangeEncoderBufferResult//g' "$rs"
	fi

	if [[ "$(grep -o 'WriteCountingWriterResult' "$rs")" != "" ]]; then
		sed -i 's/WriteCountingWriterResult,//g' "$rs"
		sed -i 's/, WriteCountingWriterResult//g' "$rs"
		sed -i 's/WriteCountingWriterResult//g' "$rs"
	fi

	if [[ "$(grep -o 'WriteSelfResult' "$rs")" != "" ]]; then
		sed -i 's/WriteSelfResult,//g' "$rs"
		sed -i 's/, WriteSelfResult//g' "$rs"
		sed -i 's/WriteSelfResult//g' "$rs"
	fi

	if [[ "$(grep -o 'ReadExactResult' "$rs")" != "" ]]; then
		sed -i 's/ReadExactResult,//g' "$rs"
		sed -i 's/, ReadExactResult//g' "$rs"
		sed -i 's/ReadExactResult//g' "$rs"
	fi

	if [[ "$(grep -o 'ReadSelfExactResult' "$rs")" != "" ]]; then
		sed -i 's/ReadSelfExactResult,//g' "$rs"
		sed -i 's/, ReadSelfExactResult//g' "$rs"
		sed -i 's/ReadSelfExactResult//g' "$rs"
	fi

	while true; do
		if [[ "$(grep -o 'crate::io::crate::io' "$rs")" == "" ]]; then
			break
		fi
		sed -i 's/crate::io::crate::io/crate::io/g' "$rs"
	done

done
