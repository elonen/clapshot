import setuptools
from setuptools import setup


with open("README.md", "r") as f:
    long_description = f.read()

with open('requirements.txt') as f:
    install_requires = f.read()


setup(
    name='clapshot-server',

    entry_points={
        'console_scripts': [
            'clapshot-server = clapshot_server.main:main',
            'clapshot-alembic = clapshot_server.alembic:main'
        ],
    },
    data_files=[],

    version="0.2.2",
    author="Jarno Elonen",
    author_email="elonen@iki.fi",
    description="Backend server for Clapshot",
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="https://github.com/elonen/clapshot",
    packages=setuptools.find_packages() + [
        'clapshot_server',
        'clapshot_server.sqlite-migrations',
        'clapshot_server.sqlite-migrations.versions'],
    package_data={'clapshot_server': ['*.ini']},
    include_package_data=True,
    classifiers=[
        "Programming Language :: Python :: 3",
        "License :: OSI Approved :: MIT License",
        "Operating System :: OS Independent",
    ],
    python_requires='>=3.9',
    platforms='any',
    install_requires=install_requires
)
