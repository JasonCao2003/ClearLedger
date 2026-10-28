import test from 'node:test'
import assert from 'node:assert/strict'
import {
  editableYuan,
  formatMoney,
  parseYuan,
} from '../src/composables/money.ts'

test('decimal input is converted without floating point rounding', () => {
  assert.equal(parseYuan('0.29'), '29')
  assert.equal(parseYuan('123.4'), '12340')
  assert.equal(parseYuan(' 001.01 '), '101')
  assert.equal(parseYuan('92233720368547758.07'), '9223372036854775807')
})

test('invalid amounts and i64 overflow cannot be submitted', () => {
  for (const value of [
    '0',
    '-1',
    '1.001',
    '1e3',
    'NaN',
    '1,000',
    '',
    '.5',
    '92233720368547758.08',
  ]) {
    assert.throws(() => parseYuan(value), undefined, value)
  }
})

test('large amounts and negative refund balance preserve exact cents', () => {
  assert.equal(formatMoney('9223372036854775807'), '92,233,720,368,547,758.07')
  assert.equal(formatMoney('-3000'), '−30.00')
  assert.equal(formatMoney('0'), '0.00')
  assert.equal(editableYuan('29'), '0.29')
  assert.equal(parseYuan(editableYuan('9007199254740993')), '9007199254740993')
})
