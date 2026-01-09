from setuptools import setup, find_packages

setup(
    name='clapshot_organizer_basic_folders',
    version='0.9.2',
    packages=find_packages(),
    include_package_data=True,

    install_requires=['clapshot_grpc'],
    package_data={
        'organizer': ['py.typed'],
    },

    entry_points={
        'console_scripts': [
            'clapshot-organizer-basic-folders=organizer.main:main',
        ],
    },
)
