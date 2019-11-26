# estunnel

[![Build](https://travis-ci.org/wfxr/estunnel.svg)](https://travis-ci.org/wfxr/estunnel)
[![Platform](https://img.shields.io/badge/Platform-OSX%20|%20Linux-orange.svg)][release]
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](https://wfxr.mit-license.org/2019)

*estunnel* is a CLI tool written by rust for downloading data from [elasticsearch](https://github.com/elastic/elasticsearch).

## Command-line options
```
USAGE:
    estunnel pull [OPTIONS] --index <index> --query <query>

FLAGS:
        --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -b, --batch <batch>      Scroll batch size. Size in query will be used if null.
    -h, --host <host>        ElasticSearch host url [default: http://localhost:9200]
    -i, --index <index>      Target index name(or alias)
    -o, --output <output>    File path for output [default: /dev/stdout]
    -q, --query <query>      File path for query body
    -s, --slice <slice>      Scroll slice count [default: 1]
        --ttl <ttl>          Scroll session ttl [default: 1m]
    -u, --user <user>        Username for http basic authorization
```

This is the output of `estunnel pull --help`.

## License

[MIT](https://wfxr.mit-license.org/2019) (c) Wenxuan Zhang

[release]: https://github.com/wfxr/estunnel/releases
