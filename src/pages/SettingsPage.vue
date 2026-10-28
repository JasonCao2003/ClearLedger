<script setup lang="ts">
import { NButton, NSwitch } from 'naive-ui'
import { useAppStore } from '../stores/app'
const app = useAppStore()
</script>
<template>
  <div class="settings-grid">
    <section class="panel">
      <div class="panel-heading">
        <h2>本地服务</h2>
        <n-button :loading="app.loading" @click="app.refreshStatus()"
          >刷新状态</n-button
        >
      </div>
      <dl class="settings-list">
        <div>
          <dt>运行状态</dt>
          <dd>
            {{
              app.loading
                ? '连接中…'
                : app.status?.databaseReady
                  ? 'SQLite 已就绪'
                  : '未连接'
            }}
          </dd>
        </div>
        <div>
          <dt>应用版本</dt>
          <dd>{{ app.status?.appVersion ?? '—' }}</dd>
        </div>
        <div>
          <dt>数据库版本</dt>
          <dd>{{ app.status?.schemaVersion ?? '—' }}</dd>
        </div>
        <div>
          <dt>账本币种</dt>
          <dd>{{ app.status?.currency ?? '—' }}</dd>
        </div>
        <div>
          <dt>数据库位置</dt>
          <dd class="file-path">
            {{ app.status?.databasePath ?? '请在桌面应用中查看' }}
          </dd>
        </div>
      </dl>
      <p v-if="app.error" class="muted">
        错误码：{{ app.error.code }} · {{ app.error.traceId }}
      </p>
    </section>
    <section class="panel">
      <div class="panel-heading"><h2>显示偏好</h2></div>
      <div class="preference-row">
        <div>
          <h3>深色主题</h3>
          <p class="muted">当前会话立即生效</p>
        </div>
        <n-switch
          :value="app.theme === 'dark'"
          aria-label="深色主题"
          @update:value="app.toggleTheme()"
        />
      </div>
      <div class="development-note">
        <h3>备份与恢复</h3>
        <p>将在记账闭环阶段接入完整备份。当前界面不提供尚未实现的备份操作。</p>
      </div>
    </section>
  </div>
</template>
