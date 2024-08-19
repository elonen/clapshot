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
    version='0.8.2',
    packages=find_packages(),
    include_package_data=True,

    install_requires=[],
    package_data={
        '': ['deps/*.tar.gz'],  # locally built clapshot_grpc
    },

    cmdclass={
        'install': CustomInstall,
    },
    entry_points='''
        [console_scripts]
        clapshot-organizer-basic-folders=organizer.main:main
    ''',
)
