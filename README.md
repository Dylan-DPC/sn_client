# safe_client_libs

|Linux/OS X|Windows|Issues|
|:--------:|:-----:|:----:|
|[![Build Status](https://travis-ci.org/maidsafe/safe_client_libs.svg?branch=master)](https://travis-ci.org/maidsafe/safe_client_libs)|[![Build status](https://ci.appveyor.com/api/projects/status/qyvxnojplcwcey4l/branch/master?svg=true)](https://ci.appveyor.com/project/MaidSafe-QA/safe-client-libs/branch/master)|[![Stories in Ready](https://badge.waffle.io/maidsafe/safe_client_libs.png?label=ready&title=Ready)](https://waffle.io/maidsafe/safe_client_libs)|

| [MaidSafe website](https://maidsafe.net) | [SAFE Dev Forum](https://forum.safedev.org) | [SAFE Network Forum](https://safenetforum.org) |
|:-------:|:-------:|:-------:|

This is the project workspace. Please refer to individual members for details:

- [safe_core](safe_core/README.md)
- [safe_authenticator](safe_authenticator/README.md)
- [safe_app](safe_app/README.md)

## Building from Source

### Docker

One option for building the libraries is to use Docker. If you're not familiar with it, the official [getting started guide](https://docs.docker.com/get-started/) is a good general introduction to the concepts and terminology. Docker provides a mechanism to build SCL without having to install anything else in the build environment.

Though it would be possible to use Docker in a day-to-day development workflow, it has certain practicalities that probably don't make it as suitable for that. It's more useful if you want to reproduce our remote build environment locally, usually for debugging purposes. If you want to get a shell inside the container, run the `make debug` target.

This repository provides a Dockerfile that defines a container with the prerequisites necessary for building SCL. You can build the container by running `make build-container`. The container build process runs a build of the safe_authenticator and safe_app libraries. This is so it can resolve and compile all the dependencies; subsequent runs of the container will then have these dependencies cached. Since the dependencies for the non-mocked versions of safe_authenticator and safe_app are a superset of the mocked versions, the container also has all the dependencies necessary for performing a mocked build.

After building the container, you can build the SCL libraries just by running `make` at the root of this repository, or you can also run a build with tests by running `make tests`. If you want to run a build with mocked routing, run `make build-mock`. These targets wrap the details of using the container, making it much easier to work with. The current source directory is mounted into the container with the correct permissions, and the artifacts produced by the build are copied out of the container and placed at the root of this repository. See the Makefile for more details.

## Testing

When making changes, please run the appropriate tests, described below, before submitting a PR.

### Mock routing

If a change is minor or does not involve potential breakage in data or API compatibility, it is not necessary to test against the actual network. In this case, it is enough to run unit tests using mock routing and make sure that they all pass. To do this, you will have to go to the crate you wish to test (e.g. the `safe_core` directory) and execute the following:

`cargo test --release --features=use-mock-routing`

This will run all unit tests for the crate. We run tests in release mode (indicated by the `--release` flag) in order to catch rare FFI bugs that may not manifest themselves in debug mode. Debug mode is also unoptimized and can take an inordinate amount of time to run tests.

### Real network

When in doubt, perform an integration test against the real network, in addition to mock routing tests above. This will test various network operations, so please make sure you have some available account mutations. The steps:

* You will need a SAFE network account. You can register one from within the [SAFE Browser](https://github.com/maidsafe/safe_browser/releases).
* Make sure you are able to login to your account via the SAFE Browser.
* Navigate to the `tests` directory.
* Open `tests.config` and set your account locator and password. **Do not commit this.** Alternatively, you can set the environment variables `TEST_ACC_LOCATOR` and `TEST_ACC_PASSWORD`.
* Run `cargo test authorisation_and_revocation --release -- --ignored --nocapture` and make sure that no errors are reported.

#### Binary compatibility of data

You may need to test whether your changes affected the binary compatibility of data on the network. This is necessary when updating compression or serialization dependencies to make sure that existing data can still be read using the new versions of the libraries.

* Set your account locator and password as per the instructions above.
* Run the following command on the **master** branch: `cargo test write_data -- --ignored --nocapture`
* On the branch with your changes, ensure the following command completes successfully: `cargo test read_data -- --ignored --nocapture`

### Viewing debug logs

The codebase contains instrumentation statements that log potentially useful debugging information, at different priorities. Such statements include the macros `debug!` and `trace!`, and more can be found at the documentation for the [log crate](https://docs.rs/log). Feel free to add trace calls to the code when debugging.

In order to view the output of trace calls you will need to initialize logging at the beginning of your test:

```rust
unwrap!(maidsafe_utilities::log::init(true));
```

Then you will need to set the `RUST_LOG` environment variable to the desired logging level for the desired modules or crates. To view trace calls for `safe_authenticator`, for example, you may do this:

```shell
export RUST_LOG=safe_authenticator=trace
```

You could also set `RUST_LOG=trace` which will output *all* trace logs, but this may produce far more data than desired. For more information please see the documentation for the `log` module in our crate [maidsafe_utilities](https://docs.rs/maidsafe_utilities).

## License

This SAFE Network Software is licensed under the General Public License (GPL), version 3 ([LICENSE](LICENSE) http://www.gnu.org/licenses/gpl-3.0.en.html).
