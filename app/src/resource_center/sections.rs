use warp_core::context_flag::ContextFlag;
use warp_core::features::FeatureFlag;
use warp_i18n::t;
use warpui::ViewContext;

use super::{
    ContentItem, ContentSectionData, FeatureItem, FeatureSection, FeatureSectionData,
    ResourceCenterMainView, Section, Tip, TipAction, TipHint,
};

pub fn sections(ctx: &mut ViewContext<ResourceCenterMainView>) -> Vec<Section> {
    let mut sections = vec![Section::Changelog()];

    if FeatureFlag::AvatarInTabBar.is_enabled() {
        return sections;
    }

    let get_started = FeatureSectionData {
        section_name: FeatureSection::GettingStarted,
        items: vec![
            FeatureItem::new(
                t!("resource-center-tip-create-block-title"),
                t!("resource-center-tip-create-block-desc"),
                Tip::Hint(TipHint::CreateBlock),
                ctx,
            ),
            FeatureItem::new(
                t!("resource-center-tip-navigate-blocks-title"),
                t!("resource-center-tip-navigate-blocks-desc"),
                Tip::Hint(TipHint::BlockSelect),
                ctx,
            ),
            FeatureItem::new(
                t!("resource-center-tip-block-action-title"),
                t!("resource-center-tip-block-action-desc"),
                Tip::Hint(TipHint::BlockAction),
                ctx,
            ),
            FeatureItem::new(
                t!("resource-center-tip-command-palette-title"),
                t!("resource-center-tip-command-palette-desc"),
                Tip::Action(TipAction::CommandPalette),
                ctx,
            ),
            FeatureItem::new(
                t!("resource-center-tip-theme-title"),
                t!("resource-center-tip-theme-desc"),
                Tip::Action(TipAction::ThemePicker),
                ctx,
            ),
        ],
    };
    sections.push(Section::Feature(get_started));

    let maximize_warp = FeatureSectionData {
        section_name: FeatureSection::MaximizeWarp,
        items: maximize_warp_items(ctx),
    };
    sections.push(Section::Feature(maximize_warp));

    let advanced_setup = ContentSectionData {
        section_name: FeatureSection::AdvancedSetup,
        items: vec![
            ContentItem {
                title: t!("resource-center-content-prompt-title"),
                description: t!("resource-center-content-prompt-desc"),
                url: "https://docs.warp.dev/terminal/appearance/prompt",
                button_label: t!("resource-center-content-prompt-button"),
            },
            ContentItem {
                title: t!("resource-center-content-ide-title"),
                description: t!("resource-center-content-ide-desc"),
                url: "https://docs.warp.dev/terminal/integrations-and-plugins",
                button_label: t!("resource-center-content-ide-button"),
            },
            ContentItem {
                title: t!("resource-center-content-blog-title"),
                description: t!("resource-center-content-blog-desc"),
                url: "https://www.warp.dev/blog/how-warp-uses-warp",
                button_label: t!("resource-center-content-blog-button"),
            },
        ],
    };
    sections.push(Section::Content(advanced_setup));

    sections
}

fn maximize_warp_items(ctx: &mut ViewContext<ResourceCenterMainView>) -> Vec<FeatureItem> {
    let mut maximize_warp_items = vec![];

    maximize_warp_items.push(FeatureItem::new(
        t!("resource-center-tip-command-search-title"),
        t!("resource-center-tip-command-search-desc"),
        Tip::Action(TipAction::CommandSearch),
        ctx,
    ));

    maximize_warp_items.push(FeatureItem::new(
        t!("resource-center-tip-ai-command-search-title"),
        t!("resource-center-tip-ai-command-search-desc"),
        Tip::Action(TipAction::AiCommandSearch),
        ctx,
    ));

    if ContextFlag::CreateNewSession.is_enabled() {
        maximize_warp_items.push(FeatureItem::new(
            t!("resource-center-tip-split-panes-title"),
            t!("resource-center-tip-split-panes-desc"),
            Tip::Action(TipAction::SplitPane),
            ctx,
        ));
    }

    if ContextFlag::LaunchConfigurations.is_enabled() {
        maximize_warp_items.push(FeatureItem::new(
            t!("resource-center-tip-launch-config-title"),
            t!("resource-center-tip-launch-config-desc"),
            Tip::Action(TipAction::SaveNewLaunchConfig),
            ctx,
        ));
    }

    maximize_warp_items
}
