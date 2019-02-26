from setuptools import setup
from setuptools_rust import RustExtension
import sys


def get_py_version_cfgs():
    # For now each Cfg Py_3_X flag is interpreted as "at least 3.X"
    version = sys.version_info[0:2]
    if version[0] == 2:
        raise SystemError("python2 not work")
        # return ["--cfg=Py_2"]
    py3_min = 5
    out_cfg = []
    for minor in range(py3_min, version[1] + 1):
        out_cfg.append("--cfg=Py_3_%d" % minor)
    return out_cfg


install_requires = list()
with open("requirements.txt", mode="r") as fp:
    for line in fp.read().split("\n"):
        if len(line) > 5:
            install_requires.append(line)


main_version = '0.1.0-unknown'
with open("Cargo.toml", mode="r") as fp:
    for line in fp.read().split("\n"):
        if not line.startswith("version"):
            continue
        _, main_version = line.split("=", 2)
        main_version = main_version.lstrip().rstrip()
        main_version = main_version[1:]
        main_version = main_version[:-1]
        break

setup(
    name="bc4py_extension",
    version=main_version,
    classifiers=[
        "License :: OSI Approved :: MIT License",
        "Programming Language :: Python",
        "Programming Language :: Rust",
    ],
    rust_extensions=[
        RustExtension(
            "bc4py_extension",
            "Cargo.toml",
            rustc_flags=get_py_version_cfgs()
        )
    ],
    install_requires=install_requires,
    include_package_data=True,
    zip_safe=False
)
