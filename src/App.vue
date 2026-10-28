<script setup lang="ts">
import { computed, onMounted, watchEffect } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  NConfigProvider,
  NGlobalStyle,
  darkTheme,
  type GlobalThemeOverrides,
} from 'naive-ui'
import { ArrowUpRight, BookOpen, Menu, Moon, Sun, X } from 'lucide-vue-next'
import { navigation } from './app/router'
import { useAppStore } from './stores/app'
import NavIcon from './components/NavIcon.vue'

const app = useAppStore()
const route = useRoute()
const router = useRouter()
const current = computed(
  () => navigation.find((item) => item.path === route.path) ?? navigation[0],
)
const overrides: GlobalThemeOverrides = {
  common: {
    primaryColor: '#315bce',
    primaryColorHover: '#254cb8',
    borderRadius: '8px',
    fontFamily: 'Inter, "Segoe UI", "Microsoft YaHei", sans-serif',
  },
}
onMounted(() => {
  void app.refreshStatus()
})
watchEffect(() => {
  document.documentElement.dataset.theme = app.theme
})
router.afterEach(() => {
  app.sidebarOpen = false
})
</script>

<template>
  <n-config-provider
    :theme="app.theme === 'dark' ? darkTheme : null"
    :theme-overrides="overrides"
  >
    <n-global-style />
    <a class="skip-link" href="#main-content">跳转到主内容</a>
    <div class="app-shell">
      <button
        v-if="app.sidebarOpen"
        class="sidebar-backdrop"
        aria-label="关闭导航"
        @click="app.sidebarOpen = false"
      />
      <aside
        class="sidebar"
        :class="{ open: app.sidebarOpen }"
        aria-label="主导航"
      >
        <div class="brand">
          <span class="brand-mark"
            ><BookOpen :size="23" aria-hidden="true"
          /></span>
          <div>清账<small>CLEARLEDGER</small></div>
          <button
            class="icon-button mobile-only"
            aria-label="关闭导航"
            @click="app.sidebarOpen = false"
          >
            <X :size="20" />
          </button>
        </div>
        <p class="nav-caption">我的账本</p>
        <nav>
          <router-link
            v-for="item in navigation"
            :key="item.path"
            :to="item.path"
            :class="{ active: route.path === item.path }"
            :aria-current="route.path === item.path ? 'page' : undefined"
            ><NavIcon :name="item.icon" /><span>{{ item.label }}</span
            ><span v-if="route.path === item.path" class="nav-dot"
          /></router-link>
        </nav>
        <div class="sidebar-bottom">
          <span
            class="status-dot"
            :class="{ connected: app.status?.databaseReady }"
          />
          <div>
            {{
              app.status?.databaseReady
                ? '本地数据库已连接'
                : '等待连接本地服务'
            }}<small>数据保存在你的设备上</small>
          </div>
        </div>
      </aside>
      <div class="workspace">
        <header class="topbar">
          <div class="breadcrumb">
            <button
              class="icon-button mobile-only"
              aria-label="展开导航"
              @click="app.sidebarOpen = true"
            >
              <Menu :size="20" /></button
            ><span>个人空间</span><span>/</span
            ><strong>{{ current.label }}</strong>
          </div>
          <div class="top-actions">
            <span class="local-badge">本地 · 离线优先</span
            ><button
              class="icon-button"
              :aria-label="
                app.theme === 'light' ? '切换深色主题' : '切换浅色主题'
              "
              @click="app.toggleTheme"
            >
              <Moon v-if="app.theme === 'light'" :size="18" /><Sun
                v-else
                :size="18"
              />
            </button>
          </div>
        </header>
        <main id="main-content" tabindex="-1">
          <div class="page-heading">
            <div>
              <p class="eyebrow">
                {{ app.status?.bookName ?? '我的个人账本' }}
              </p>
              <h1>{{ current.label }}</h1>
              <p class="subtitle">{{ current.subtitle }}</p>
            </div>
            <router-link
              v-if="route.path === '/'"
              to="/imports"
              class="primary-link"
              >前往导入中心<ArrowUpRight :size="17" aria-hidden="true"
            /></router-link>
          </div>
          <div v-if="app.error" class="connection-notice" role="status">
            {{ app.error.message
            }}<button
              v-if="app.error.code !== 'DESKTOP_REQUIRED'"
              :disabled="app.loading"
              @click="app.refreshStatus()"
            >
              {{ app.loading ? '连接中…' : '重试连接' }}
            </button>
          </div>
          <router-view />
          <footer>
            清账 <span>·</span>
            {{ app.status ? `v${app.status.appVersion}` : '框架预览' }}
            <span>·</span> 每笔交易，都有依据
          </footer>
        </main>
      </div>
    </div>
  </n-config-provider>
</template>
