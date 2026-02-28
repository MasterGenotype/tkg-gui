# Maintainer: MasterGenotype <https://github.com/MasterGenotype>
pkgname=tkg-gui
pkgver=0.1.0
pkgrel=1
pkgdesc="A graphical interface for building custom Linux kernels using the linux-tkg build system"
arch=('x86_64')
url="https://github.com/MasterGenotype/tkg-gui"
license=('unknown')
depends=('gcc-libs' 'glibc')
makedepends=('cargo' 'git')
source=("git+${url}.git")
sha256sums=('SKIP')

prepare() {
  cd "$pkgname"
  git submodule update --init --recursive
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "$pkgname"
  export RUSTUP_TOOLCHAIN=stable
  export CARGO_TARGET_DIR=target
  cargo build --frozen --release
}

package() {
  cd "$pkgname"
  install -Dm755 "target/release/tkg-gui" "$pkgdir/usr/bin/tkg-gui"

  # Install submodules alongside the binary's expected location
  install -dm755 "$pkgdir/usr/share/$pkgname"
  cp -a submodules "$pkgdir/usr/share/$pkgname/submodules"
}
