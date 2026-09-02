import type { Meta, StoryObj } from '@storybook/svelte-vite'
import StateCatalog from './StateCatalog.svelte'

const meta = {
  title: 'Wave 10/State catalog',
  component: StateCatalog,
  args: { state: 'ready', locale: 'ru', longContent: false, reducedMotion: false },
  parameters: { layout: 'fullscreen' },
} satisfies Meta<typeof StateCatalog>

export default meta
type Story = StoryObj<typeof meta>

export const Empty: Story = { args: { state: 'empty' } }
export const Loading: Story = { args: { state: 'loading' } }
export const Error: Story = { args: { state: 'error' } }
export const LongContent: Story = { args: { state: 'ready', longContent: true } }
export const Localized: Story = { args: { state: 'empty', locale: 'en' } }
export const Mobile: Story = {
  args: { state: 'ready' },
  globals: { viewport: { value: 'mobile1', isRotated: false } },
}
export const ReducedMotion: Story = {
  args: { state: 'loading', reducedMotion: true },
  parameters: { reducedMotion: 'reduce' },
}
