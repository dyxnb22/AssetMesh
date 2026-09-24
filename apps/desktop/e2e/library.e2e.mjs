import assert from 'node:assert/strict';

describe('real Tauri desktop workflow', () => {
  it('creates, searches, edits, and reads back an asset over real IPC', async () => {
    await $('[data-testid="nav-all"]').waitForDisplayed();
    await $('[data-testid="nav-media"]').click();
    await $('[data-testid="new-asset-button"]').click();
    await $('[data-testid="create-media-form"]').waitForDisplayed();
    await $('[data-testid="media-create-title-input"]').setValue('Interstellar E2E');
    await $('[data-testid="media-create-type-select"]').selectByAttribute('value', 'movie');
    await $('[data-testid="submit-create-media-button"]').click();

    await $('[data-testid="asset-detail-view"]').waitForDisplayed();
    assert.match(await $('[data-testid="asset-detail-view"] h2').getText(), /Interstellar E2E/);
    await $('[data-testid="edit-media-metadata-button"]').click();
    await $('[data-testid="media-notes-input"]').setValue('Written through real Tauri IPC');
    await $('[data-testid="save-media-button"]').click();
    await $('[data-testid="media-edit-form"]').waitForDisplayed({ reverse: true });
    assert.match(await $('[data-testid="asset-detail-view"]').getText(), /Written through real Tauri IPC/);

    await $('[aria-label="Close detail view"]').click();
    await $('[data-testid="asset-detail-view"]').waitForDisplayed({ reverse: true });
    await $('input[placeholder*="Search assets"]').setValue('Interstellar E2E');
    await $('[role="table"][aria-label="Assets"] .asset-row').waitForDisplayed();
    assert.match(await $('[role="table"][aria-label="Assets"] .asset-row').getText(), /Interstellar E2E/);
    await $('[role="table"][aria-label="Assets"] .asset-row').doubleClick();
    await $('[data-testid="asset-detail-view"]').waitForDisplayed();
    assert.match(await $('[data-testid="asset-detail-view"]').getText(), /Written through real Tauri IPC/);
  });
});
