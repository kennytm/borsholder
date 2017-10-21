borsholder
==========

[![Build Status](https://travis-ci.org/kennytm/borsholder.svg?branch=master)](https://travis-ci.org/kennytm/borsholder)
[![crates.io](https://img.shields.io/crates/v/borsholder.svg)](https://crates.io/crates/borsholder)

**borsholder** is a dashboard for monitoring the [Rust compiler repository]'s pull requests status.
It is a combination of rust-lang/rust's [Homu queue] with useful information (labels, last comment,
CI status, etc) obtained from the GitHub API.

![](doc/preview.png)

Installing
----------

1. Download [Rust].
2. Install **borsholder** by typing in the command line:

    ```sh
    cargo install borsholder
    ```

3. Register a GitHub account.
4. Create a [personal access token]. You do not need to enable any scopes or permissions.
5. Start **borsholder**:

    ```sh
    borsholder --token «your_personal_access_token»
    ```

6. Navigate to <http://127.0.0.1:55727> in your browser.

[Rust]: https://rustup.rs/
[Rust compiler repository]: https://github.com/rust-lang/rust
[Homu queue]: https://buildbot2.rust-lang.org/homu/queue/rust
[personal access token]: https://help.github.com/articles/creating-a-personal-access-token-for-the-command-line/