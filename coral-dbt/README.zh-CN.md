# 使用 coral-dbt 物化模式

> 🌐 语言版本：[English](README.md) | **简体中文**

本模块为 dbt 实现了一种新的物化模式：`incremental_maintenance`。该物化模式可以原地替代 `table` 物化模式，但它不会每次都重建整张表并从零计算数据，而是**增量维护**这些表。其原理是借助 Coral 将输入 SQL 重写为增量版本：只消费输入表的增量变更，计算对输出表的增量更新，然后合并进去。更多细节请参阅这份 [幻灯片](https://www.slideshare.net/walaa_eldin_moustafa/incremental-view-maintenance-with-coral-dbt-and-iceberg)。

> 注意：本项目目前仍在开发中（WIP）。

## 环境准备

1. 本包中的物化模式需要对 `dbt-core` 源码做少量修改。先将 `dbt-core` 克隆到本地并搭建起来，创建并激活一个 Python 虚拟环境。
2. 在本地的 `dbt-core` 中修改 `core/dbt/context/base.py`，加入如下内容：
```
import requests

def get_requests_module_context() -> Dict[str, Any]:
    context_exports = ["get", "post"]

    return {name: getattr(requests, name) for name in context_exports}

def get_context_modules() -> Dict[str, Dict[str, Any]]:
    return {
        "pytz": get_pytz_module_context(),
        "datetime": get_datetime_module_context(),
        "re": get_re_module_context(),
        "itertools": get_itertools_module_context(),
        "requests": get_requests_module_context(),
    }
```

3. 在你的 dbt 项目中创建或修改 `packages.yml`，把本包加进去，然后执行 `dbt deps` 安装：

```
packages:
  - git: "https://github.com/linkedin/coral.git"
    revision: master
    subdirectory: coral-dbt/src/main/resources
```

4. 修改 `dbt_project.yaml`，加入下面这行（冒号后面什么都不写是故意的）：
```
query-comment:
```
5. 根据主项目 README 的指引启动 Coral Service。默认地址为 [http://localhost:8080](http://localhost:8080)。如果需要修改，可以二选一：
   * 在 `dbt_project.yaml` 中声明 `coral_url` 变量：
   ```
    vars:
      coral_url: <your_coral_url>
    ```
   * 或者把本包克隆到本地（并将 `packages.yml` 指向你自己的版本），修改 `default/utils/configs.sql` 中的 `default_coral_url`：
   ```
    {% set default_coral_url = <your_coral_url> %}
    ```

## 进一步配置
### 增量维护
在你的模型中，通过 `table_names` 配置项声明该查询依赖的表名。一个示例模型如下：
```
{{
  config(
    materialized='incremental_maintenance',
    table_names=['db.t1', 'db.t2'],
  )
}}

SELECT * FROM db.t1 UNION SELECT * FROM db.t2
```

## 测试

### 运行测试

这些测试会作为 Gradle 构建（`./gradlew build`）的一部分自动运行，也可以手动触发 —— `cd` 到 `src/main/resources/tests/` 目录下执行：

```
python3 -m unittest -v
```

更多运行测试的细节，请参考 Python `unittest` [模块文档](https://docs.python.org/3/library/unittest.html)。

### 编写测试

建议使用 `unittest` 模块编写测试，这样就可以在构建流程中自动跑起来。
