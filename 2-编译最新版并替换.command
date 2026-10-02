#!/bin/zsh
# 双击运行:编译最新代码,并把 /Applications 里的 AssetMesh.app 替换成最新版。
# 完成后双击「1-打开AssetMesh.command」启动。
cd "$(dirname "$0")" || exit 1

fail() {
  echo
  echo "❌ $1"
  echo
  echo "按回车键关闭窗口…"
  read
  exit 1
}

echo "==> [1/3] 安装依赖(已是最新则秒过)…"
npm install --no-audit --no-fund > /dev/null 2>&1 || fail "npm install 失败"

echo "==> [2/3] 编译正式版(第一次或大改动可能需要几分钟,请勿关闭窗口)…"
npm run --workspace=apps/desktop tauri build -- --bundles app > /tmp/assetmesh-build.log 2>&1
if [ $? -ne 0 ]; then
  echo
  tail -40 /tmp/assetmesh-build.log
  fail "编译失败,错误见上方(完整日志在 /tmp/assetmesh-build.log)"
fi

APP_SRC="target/release/bundle/macos/AssetMesh.app"
[ -d "$APP_SRC" ] || fail "没找到构建产物 $APP_SRC"

# 为本地应用包补齐签名封装,验证后再替换安装版。
codesign --force --deep --sign - "$APP_SRC" > /dev/null 2>&1 || fail "本地应用签名失败"
codesign --verify --deep --strict "$APP_SRC" > /dev/null 2>&1 || fail "应用签名验证失败"

echo "==> [3/3] 替换 /Applications/AssetMesh.app…"
INSTALL_STAGE=$(mktemp -d /Applications/.assetmesh-install.XXXXXXXX) || fail "无法创建安装暂存目录"
trap 'rm -rf "$INSTALL_STAGE"' EXIT
STAGED_APP="$INSTALL_STAGE/AssetMesh.app"
ditto "$APP_SRC" "$STAGED_APP" || fail "无法暂存新应用"
codesign --verify --deep --strict "$STAGED_APP" > /dev/null 2>&1 || fail "暂存应用签名验证失败"
APP_EXECUTABLE=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$STAGED_APP/Contents/Info.plist") || fail "无法读取应用入口"
osascript -e 'quit app "AssetMesh"' > /dev/null 2>&1
for attempt in {1..50}; do
  pgrep -x "$APP_EXECUTABLE" > /dev/null || break
  sleep 0.2
done
pgrep -x "$APP_EXECUTABLE" > /dev/null && fail "AssetMesh 尚未退出，请退出应用后重新运行"
PREVIOUS_APP="$PWD/target/previous-bundles/AssetMesh.previous.app"
mkdir -p "$(dirname "$PREVIOUS_APP")" || fail "无法创建旧版保留目录"
if [ -d "/Applications/AssetMesh.app" ]; then
  rm -rf "$PREVIOUS_APP"
  mv "/Applications/AssetMesh.app" "$PREVIOUS_APP" ||
    fail "无法移动旧应用(检查是否被 Finder/安全软件占用)"
fi
mv "$STAGED_APP" /Applications/AssetMesh.app || {
  [ -d "$PREVIOUS_APP" ] && mv "$PREVIOUS_APP" "/Applications/AssetMesh.app"
  fail "安装新应用失败,已恢复旧版"
}

echo
echo "✅ 已更新到最新版。双击「1-打开AssetMesh.command」启动。"
echo
echo "按回车键关闭窗口…"
read
