#!/bin/zsh
# 双击运行:打开已安装的 AssetMesh(用「2-编译最新版并替换」更新后,这里打开的就是最新版)
open -a AssetMesh
if [ $? -ne 0 ]; then
  echo "没找到已安装的 AssetMesh。"
  echo "先双击「2-编译最新版并替换.command」,完成后再打开。"
  echo
  echo "按回车键关闭窗口…"
  read
fi
