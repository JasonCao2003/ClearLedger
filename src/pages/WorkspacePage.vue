<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import NavIcon from '../components/NavIcon.vue'
import { navigation } from '../app/router'
const route = useRoute()
const item = computed(() =>
  navigation.find((entry) => entry.path === route.path)!,
)
const descriptions: Record<string, string> = {
  '/transactions': '这里将支持手工记账、条件筛选、分类编辑和原始凭证追溯。',
  '/imports': '这里将支持账单文件选择、导入预览、校验报告和后台任务进度。',
  '/review': '这里将集中处理疑似重复、退款关联和金额性质不明确的记录。',
  '/rules': '这里将管理分类、商户别名和带预览的分类规则。',
  '/accounts': '这里将管理资金账户、支付渠道以及二者之间的映射。',
  '/quality': '这里将展示来源覆盖、字段异常及银行流水校验结果。',
}
</script>
<template>
  <section class="panel workspace-empty">
    <span class="large-icon"><NavIcon :name="item.icon" /></span
    ><span class="subtle-tag">功能待接入</span>
    <h2>{{ item.label }}</h2>
    <p>{{ descriptions[route.path] }}</p>
    <p class="muted">页面框架已就绪，当前尚未实现业务操作。</p>
    <router-link to="/" class="text-link">返回收支总览 →</router-link>
  </section>
</template>
