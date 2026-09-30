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

echo "==> [3/3] 替换 /Applications/AssetMesh.app…"
osascript -e 'quit app "AssetMesh"' > /dev/null 2>&1
sleep 1
if [ -d "/Applications/AssetMesh.app" ]; then
  rm -rf "/Applications/AssetMesh.app.bak"
  mv "/Applications/AssetMesh.app" "/Applications/AssetMesh.app.bak" ||
    fail "无法移动旧应用(检查是否被 Finder/安全软件占用)"
fi
cp -R "$APP_SRC" /Applications/ || {
  [ -d "/Applications/AssetMesh.app.bak" ] && mv "/Applications/AssetMesh.app.bak" "/Applications/AssetMesh.app"
  fail "复制新应用失败,已恢复旧版"
}
rm -rf "/Applications/AssetMesh.app.bak"

echo
echo "✅ 已更新到最新版。双击「1-打开AssetMesh.command」启动。"
echo
echo "按回车键关闭窗口…"
read
