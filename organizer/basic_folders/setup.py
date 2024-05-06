from setuptools import setup, find_packages
from setuptools.command.install import install
import subprocess
import sys


class CustomInstall(install):
    def run(self):
        # Ensure the local package is installed first
        subprocess.check_call([sys.executable, '-m', 'pip', 'install', 'deps/clapshot_grpc-0.0.0+dev.tar.gz'])
        # Run the standard setuptools install
        install.run(self)

setup(
    name='clapshot_organizer_basic_folders',
    version='0.6.0',
    packages=find_packages(),
    include_package_data=True,

    install_requires=[
        'wheel',
        'docopt',
        'SQLAlchemy',
        'grpclib',
        'betterproto==2.0.0b6',
        'types-docopt'
    ],
    package_data={
        '': ['deps/*.tar.gz'],  # Include the local tar.gz files in the package data
    },

    cmdclass={
        'install': CustomInstall,  # Use the custom install class
    },
    entry_points='''
        [console_scripts]
        clapshot-organizer-basic-folders=main:main
    ''',
)
