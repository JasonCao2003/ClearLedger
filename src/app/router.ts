import { createRouter, createWebHashHistory } from 'vue-router'

export const navigation = [
  {
    path: '/',
    label: '收支总览',
    subtitle: '看清每一笔收支，了解资金去向',
    icon: 'overview',
  },
  {
    path: '/monthly',
    label: '月度支出',
    subtitle: '逐月查看支出走势与每日明细',
    icon: 'monthly',
  },
  {
    path: '/transactions',
    label: '交易明细',
    subtitle: '管理交易，追溯每一条原始凭证',
    icon: 'transactions',
  },
  {
    path: '/imports',
    label: '导入中心',
    subtitle: '汇集不同渠道的账单',
    icon: 'imports',
  },
  {
    path: '/review',
    label: '待核实',
    subtitle: '核对重复、退款与不确定交易',
    icon: 'review',
  },
  {
    path: '/rules',
    label: '分类与规则',
    subtitle: '让分类更一致，让整理更轻松',
    icon: 'rules',
  },
  {
    path: '/accounts',
    label: '账户与渠道',
    subtitle: '区分资金账户和支付渠道',
    icon: 'accounts',
  },
  {
    path: '/quality',
    label: '数据质量',
    subtitle: '了解账单覆盖范围与校验结果',
    icon: 'quality',
  },
  {
    path: '/settings',
    label: '设置与备份',
    subtitle: '管理本地数据与应用偏好',
    icon: 'settings',
  },
] as const

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', component: () => import('../pages/OverviewPage.vue') },
    { path: '/monthly', component: () => import('../pages/MonthlyPage.vue') },
    {
      path: '/transactions',
      component: () => import('../pages/TransactionsPage.vue'),
    },
    { path: '/review', component: () => import('../pages/ReviewPage.vue') },
    ...navigation
      .filter(
        (item) =>
          item.path !== '/' &&
          item.path !== '/settings' &&
          item.path !== '/transactions' &&
          item.path !== '/monthly' &&
          item.path !== '/review',
      )
      .map((item) => ({
        path: item.path,
        component: () => import('../pages/WorkspacePage.vue'),
        meta: { title: item.label },
      })),
    { path: '/settings', component: () => import('../pages/SettingsPage.vue') },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})
