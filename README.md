[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
![beacon for google analytics](https://ga-beacon.appspot.com/UA-88834340-1/tantivy-cli/README)


`tantivy-cli` is the project hosting the command line interface for [tantivy](https://github.com/tantivy-search/tantivy), a search engine project.


# Tutorial: Indexing Wikipedia with Tantivy CLI

## Introduction

In this tutorial, we will create a brand new index with the articles of English wikipedia in it.

## Installing the tantivy CLI.

There are a couple ways to install `tantivy-cli`.

If you are a Rust programmer, you probably have `cargo` and `rustup` installed and you can just
run `rustup run nightly cargo install tantivy-cli`. (`cargo install tantivy-cli` will work
as well if nightly is your default toolchain).

Alternatively, you can directly download a
static binary for [Linux x86 64](https://github.com/tantivy-search/tantivy-cli/releases/download/0.4.2/tantivy-cli-0.4.2-x86_64-unknown-linux-musl.tar.gz) or for [Mac OS X](https://github.com/tantivy-search/tantivy-cli/releases/download/0.4.2/tantivy-cli-0.4.2-x86_64-apple-darwin.tar.gz)
and save it in a directory on your system's `PATH`.




## Creating the index:  `new`
 
Let's create a directory in which your index will be stored.

```bash
    # create the directory
    mkdir wikipedia-index
```


We will now initialize the index and create its schema.
The [schema](https://tantivy-search.github.io/tantivy/tantivy/schema/index.html) defines
the list of your fields, and for each field:
- its name 
- its type, currently `u64`, `i64` or `str`
- how it should be indexed.

You can find more information about the latter on 
[tantivy's schema documentation page](https://tantivy-search.github.io/tantivy/tantivy/schema/index.html)

In our case, our documents will contain
* a title
* a body 
* a url

We want the title and the body to be tokenized and indexed. We also want 
to add the term frequency and term positions to our index.
(To be honest, phrase queries are not yet implemented in tantivy,
so the positions won't be really useful in this tutorial.)

Running `tantivy new` will start a wizard that will help you
define the schema of the new index.

Like all the other commands of `tantivy`, you will have to 
pass it your index directory via the `-i` or `--index`
parameter as follows:


```bash
    tantivy new -i wikipedia-index
```



Answer the questions as follows:

```none

    Creating new index 
    Let's define its schema! 



    New field name  ? title
    Text or unsigned 32-bit integer (T/I) ? T
    Should the field be stored (Y/N) ? Y
    Should the field be indexed (Y/N) ? Y
    Should the field be tokenized (Y/N) ? Y
    Should the term frequencies (per doc) be in the index (Y/N) ? Y
    Should the term positions (per doc) be in the index (Y/N) ? Y
    Add another field (Y/N) ? Y



    New field name  ? body
    Text or unsigned 32-bit integer (T/I) ? T
    Should the field be stored (Y/N) ? Y
    Should the field be indexed (Y/N) ? Y
    Should the field be tokenized (Y/N) ? Y
    Should the term frequencies (per doc) be in the index (Y/N) ? Y
    Should the term positions (per doc) be in the index (Y/N) ? Y
    Add another field (Y/N) ? Y



    New field name  ? url
    Text or unsigned 32-bit integer (T/I) ? T
    Should the field be stored (Y/N) ? Y
    Should the field be indexed (Y/N) ? N
    Add another field (Y/N) ? N

    [
    {
        "name": "title",
        "type": "text",
        "options": {
            "indexing": "position",
            "stored": true
        }
    },
    {
        "name": "body",
        "type": "text",
        "options": {
            "indexing": "position",
            "stored": true
        }
    },
    {
        "name": "url",
        "type": "text",
        "options": {
            "indexing": "unindexed",
            "stored": true
        }
    }
    ]


```

After the wizard has finished, a `meta.json` should exist in `wikipedia-index/meta.json`.
It is a fairly human readable JSON, so you can check its content.

It contains two sections:
- segments (currently empty, but we will change that soon)
- schema 

 

# Indexing the document: `index`


Tantivy's `index` command offers a way to index a json file.
The file must contain one JSON object per line.
The structure of this JSON object must match that of our schema definition.

```json
    {"body": "some text", "title": "some title", "url": "http://somedomain.com"}
```

For this tutorial, you can download a corpus with the 5 million+ English Wikipedia articles in the right format here: [wiki-articles.json (2.34 GB)](https://www.dropbox.com/s/wwnfnu441w1ec9p/wiki-articles.json.bz2?dl=0).
Make sure to decompress the file

```bash
    bunzip2 wiki-articles.json.bz2
```

If you are in a rush you can [download 100 articles in the right format here (11 MB)](http://fulmicoton.com/tantivy-files/wiki-articles-1000.json).

The `index` command will index your document.
By default it will use as 3 thread, each with a buffer size of 1GB split a
accross these threads. 


```
    cat wiki-articles.json | tantivy index -i ./wikipedia-index
```

You can change the number of threads by passing it the `-t` parameter, and the total
buffer size used by the threads heap by using the `-m`. Note that tantivy's memory usage
is greater than just this buffer size parameter.

On my computer (8 core Xeon(R) CPU X3450  @ 2.67GHz), on 8 threads, indexing wikipedia takes around 9 minutes.


While tantivy is indexing, you can peek at the index directory to check what is happening.

```bash
    ls ./wikipedia-index
```

The main file is `meta.json`.

You should also see a lot of files with a UUID as filename, and different extensions.
Our index is in fact divided in segments. Each segment acts as an individual smaller index.
Its name is simply a uuid. 

If you decided to index the complete wikipedia, you may also see some of these files disappear.
Having too many segments can hurt search performance, so tantivy actually automatically starts
merging segments. 

# Serve the search index: `serve`

Tantivy's cli also embeds a search server.
You can run it with the following command.

```
    tantivy serve -i wikipedia-index
```

By default, it will serve on port `3000`.

You can search for the top 20 most relevant documents for the query `Barack Obama` by accessing
the following [url](http://localhost:3000/api/?q=barack+obama&nhits=20) in your browser

    http://localhost:3000/api/?q=barack+obama&nhits=20

By default this query is treated as `barack OR obama`.
You can also search for documents that contains both term, by adding a `+` sign before the terms in your query.

    http://localhost:3000/api/?q=%2Bbarack%20%2Bobama&nhits=20
    
Also, `-` makes it possible to remove documents the documents containing a specific term.

    http://localhost:3000/api/?q=-barack%20%2Bobama&nhits=20
    
Finally tantivy handle phrase queries.

    http://localhost:3000/api/?q=%22barack%20obama%22&nhits=20
    

# Search the index via the command line

You may also use the `search` command to stream all documents matching a specific query.
The documents are returned in an unspecified order.

```
    tantivy search -i wikipedia-index -q "barack obama"
```

