# clapshot_grpc Python library

This directory contains a Makefile that automates the generation of Python gRPC bindings from Protocol Buffers (`.proto` files). The generated bindings are then packaged into a Python module which can be installed using `pip` by your Organizer plugin.

## Structure

- **Makefile**: Contains targets to set up a virtual environment, generate Python bindings, and package them.
- **requirements.txt**: Lists the necessary Python dependencies for the generation process.

## Usage

To generate the Python module, simply run:

```
make
```


After the build completes, you'll be provided with instructions on how to install the generated module using `pip`.

## Note

You might notice the absence of `.py` files in this directory. This is intentional, as the necessary Python files are generated on-the-fly by the Makefile.
