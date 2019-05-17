#!/usr/bin/env bash
# Building and packaging for release

set -ex

pack() {
    local tempdir
    local out_dir
    local package_name
    local gcc_prefix

    tempdir=$(mktemp -d 2>/dev/null || mktemp -d -t tmp)
    out_dir=$(pwd)
    package_name="$PROJECT_NAME-$TRAVIS_TAG-${TARGET//-unknown}"
    gcc_prefix=""

    # create a "staging" directory
    mkdir "$tempdir/$package_name"
    # mkdir "$tempdir/$package_name/autocomplete"

    # copying the main binary
    cp "target/$TARGET/release/$PROJECT_NAME" "$tempdir/$package_name/"
    "${gcc_prefix}"strip "$tempdir/$package_name/$PROJECT_NAME"

    # manpage, readme and license
    # cp "doc/$PROJECT_NAME.1" "$tempdir/$package_name"
    # cp README.md "$tempdir/$package_name"
    # cp LICENSE-MIT "$tempdir/$package_name"
    # cp LICENSE-APACHE "$tempdir/$package_name"

    # various autocomplete
    # cp target/"$TARGET"/release/build/"$PROJECT_NAME"-*/out/"$PROJECT_NAME".bash "$tempdir/$package_name/autocomplete/${PROJECT_NAME}.bash-completion"
    # cp target/"$TARGET"/release/build/"$PROJECT_NAME"-*/out/"$PROJECT_NAME".fish "$tempdir/$package_name/autocomplete"
    # cp target/"$TARGET"/release/build/"$PROJECT_NAME"-*/out/_"$PROJECT_NAME" "$tempdir/$package_name/autocomplete"

    # archiving
    pushd "$tempdir"
    tar czf "$out_dir/$package_name.tgz" "$package_name"/*
    popd
    rm -r "$tempdir"
}

main() {
    pack
}

main
