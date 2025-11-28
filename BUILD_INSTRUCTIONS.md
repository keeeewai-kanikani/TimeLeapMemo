# TimeLeapMemo ビルド手順

## 必要な環境

- Python 3.8以上
- pip (Pythonパッケージマネージャー)
- Visual Studio Build Tools または MinGW64 (C/C++コンパイラ)

## 依存ライブラリのインストール

アプリケーションが使用しているライブラリ:
```bash
pip install PyQt6 moderngl numpy nuitka
```

## ビルド手順

### 方法1: バッチファイルを使用（推奨）

1. プロジェクトフォルダを開く:
   ```
   c:\Users\yutaa\OneDrive\ドキュメント\四則和算\タイムリープメモ
   ```

2. `build_exe.bat`をダブルクリックまたはコマンドプロンプトから実行:
   ```
   build_exe.bat
   ```

3. ビルドが完了すると、`Releas\TimeLeapMemo.exe`が作成されます

### 方法2: 手動でNuitkaコマンドを実行

コマンドプロンプトで以下を実行:

```bash
cd "c:\Users\yutaa\OneDrive\ドキュメント\四則和算\タイムリープメモ"

python -m nuitka ^
    --standalone ^
    --onefile ^
    --windows-icon-from-ico=Icon\timeLeapMemoIcon_v2.ico ^
    --include-data-file=config.json=config.json ^
    --enable-plugin=pyqt6 ^
    --windows-console-mode=disable ^
    --output-dir=Releas ^
    --output-filename=TimeLeapMemo.exe ^
    --assume-yes-for-downloads ^
    TimeLeapMemo.py
```

## Nuitkaオプションの説明

| オプション | 説明 |
|-----------|------|
| `--standalone` | すべての依存関係を含める |
| `--onefile` | 単一の実行ファイルとして出力 |
| `--windows-icon-from-ico` | アイコンファイルを埋め込む |
| `--include-data-file` | データファイル(config.json)を埋め込む |
| `--enable-plugin=pyqt6` | PyQt6プラグインを有効化 |
| `--windows-console-mode=disable` | コンソールウィンドウを非表示 |
| `--output-dir` | 出力ディレクトリ |
| `--output-filename` | 出力ファイル名 |
| `--assume-yes-for-downloads` | 必要なファイルを自動ダウンロード |

## ビルド時間

初回ビルドは10〜30分程度かかる場合があります。
2回目以降はキャッシュにより高速化されます。

## トラブルシューティング

### Nuitkaがインストールされていない

```bash
pip install nuitka
```

### C/C++コンパイラが見つからない

**Visual Studio Build Toolsのインストール:**
1. https://visualstudio.microsoft.com/downloads/ から「Build Tools for Visual Studio」をダウンロード
2. インストール時に「C++によるデスクトップ開発」を選択

**または MinGW64を使用:**
```bash
pip install mingw-w64
```

### PyQt6プラグインのエラー

PyQt6が正しくインストールされているか確認:
```bash
pip install --upgrade PyQt6
```

### modernglのエラー

OpenGLドライバーが最新であることを確認してください。

### ビルドは成功したが実行できない

- Windows Defenderやウイルス対策ソフトが実行ファイルをブロックしている可能性があります
- 例外として追加するか、一時的に無効化してください

## 配布方法

`Releas\TimeLeapMemo.exe`を配布するだけで、他のPCで動作します。
Pythonのインストールは不要です。

## ファイルサイズ

完成した実行ファイルのサイズは約50〜150MB程度になります。
これはPythonランタイムとすべての依存ライブラリが含まれているためです。

## config.jsonについて

`config.json`は実行ファイルに埋め込まれますが、実行時には実行ファイルと同じフォルダに
新しい`config.json`が作成され、そちらが使用されます。
これにより、最後に開いたフォルダなどの設定が保存されます。
