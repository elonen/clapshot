from setuptools import setup, find_packages

setup(
    name='clapshot_slack_unfurl',
    version='0.1.0',
    packages=find_packages(),
    include_package_data=True,
    install_requires=[
        'slack_bolt',
        'slack_sdk',
        'Pillow',
    ],
    entry_points={
        'console_scripts': [
            'clapshot-slack-unfurl=clapshot_slack_unfurl.main:main',
        ],
    },
)
