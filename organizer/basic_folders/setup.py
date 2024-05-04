from setuptools import setup, find_packages

setup(
    name='clapshot_organizer_basic_folders',
    version='0.6.0',
    packages=find_packages(),
    include_package_data=True,
    install_requires=[],
    entry_points='''
        [console_scripts]
        clapshot-organizer-basic-folders=main:main
    ''',
)
