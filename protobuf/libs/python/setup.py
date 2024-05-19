from setuptools import setup, find_packages
setup(
    name='clapshot_grpc',
    version='0.0.0+dev',
    packages=find_packages(where='src'),
    package_dir={'': 'src'},
    install_requires=[],
    package_data={'clapshot_grpc': ['py.typed', '*.pyi']})
